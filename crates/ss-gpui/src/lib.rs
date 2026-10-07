//! GPUI desktop shell for SkillStar.
//!
//! A left sidebar switches between [`NavPage`]s. Each page is a GPUI view
//! that calls `crates/*` directly. Async domain work runs on a dedicated
//! tokio runtime; results post back into GPUI via `Entity::update` inside
//! `cx.spawn`. Tray, deep links, and a signed updater left with the Tauri
//! shell and are not in this process yet. See docs/decisions.md D-091.

mod accounts;
mod agent_icons;
mod chrome;
mod i18n;
mod layout;
#[cfg(target_os = "macos")]
mod macos_cursor;
mod marketplace;
mod my_skills;
mod nav;
mod notify;
mod os_open;
mod prefs;
mod projects;
mod settings;
mod shell;
mod skill_card;
mod skill_cards;
mod theme;
mod translation;

pub use nav::NavPage;
pub use prefs::GuiPrefs;
pub use shell::Shell;

use gpui_kit::component::Root;
use gpui_kit::*;
use std::borrow::Cow;

use layout::{WINDOW_H, WINDOW_MIN_H, WINDOW_MIN_W, WINDOW_W};

/// Asset source that layers the SkillStar app icon over gpui-kit's full
/// Lucide catalogue. Paths under `icons/` that Lucide doesn't carry
/// (the app logo) resolve here; everything else falls through.
#[derive(Clone, Copy)]
struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == "icon.png" {
            return Ok(Some(Cow::Borrowed(
                include_bytes!("../assets/icon.png") as &[u8]
            )));
        }
        if let Some(bytes) = agent_icons::load_agent_icon_path(path) {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::AllAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut out = gpui_kit::assets::AllAssets.list(path)?;
        if "icon.png".starts_with(path) {
            out.push("icon.png".into());
        }
        Ok(out)
    }
}

/// Bundled DM Sans + JetBrains Mono. Embedded via `include_bytes!` so the
/// binary carries its own type (no CDN, no system-font fallback).
static FONT_DM_SANS: &[u8] = include_bytes!("../assets/fonts/DMSans-Variable.ttf");
static FONT_DM_SANS_ITALIC: &[u8] = include_bytes!("../assets/fonts/DMSans-Variable-Italic.ttf");
static FONT_JETBRAINS_MONO: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Variable.ttf");

fn install_fonts(cx: &mut App) {
    let fonts = vec![
        Cow::Borrowed(FONT_DM_SANS),
        Cow::Borrowed(FONT_DM_SANS_ITALIC),
        Cow::Borrowed(FONT_JETBRAINS_MONO),
    ];
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        tracing::warn!("failed to register bundled fonts: {err}");
        return;
    }
    // `Theme::update` (not `global_mut`) — it re-resolves the family
    // against the font cache and refreshes windows so text re-lays out.
    gpui_kit::component::theme::Theme::update(cx, |theme| {
        theme.font_family = "DM Sans".into();
        theme.mono_font_family = "JetBrains Mono".into();
    });
}

/// The React `::-webkit-scrollbar` (`src/index.css`) is a translucent
/// white thumb on a transparent track that only shows while scrolling.
/// gpui-component's default projects an opaque mid-gray thumb that stays
/// parked on the edge. Re-projecting the same treatment onto the Base
/// scrollbar theme fixes every `overflow_y_scrollbar` call site at once.
fn install_scrollbar_theme(cx: &mut App) {
    use gpui_kit::component::scroll::ScrollbarMode;
    use gpui_kit::component::theme::Theme;

    Theme::update(cx, |theme| {
        theme.scrollbar_mode = ScrollbarMode::Scrolling;
        theme.scrollbar_thumb = hsla(0.0, 0.0, 1.0, 0.18);
        theme.scrollbar_thumb_hover = hsla(0.0, 0.0, 1.0, 0.32);
    });
}

/// Dock icon. A bare `cargo run` binary has no .app bundle, so macOS
/// falls back to the generic "exec" tile; `setApplicationIconImage`
/// overrides it at runtime with the same `assets/icon.png` the
/// sidebar logo uses.
#[cfg(target_os = "macos")]
fn install_dock_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(include_bytes!("../assets/icon.png"));
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        tracing::warn!("dock icon: NSImage failed to decode assets/icon.png");
        return;
    };
    unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
}

/// Process-wide tokio runtime for domain async calls. `installed_skill`
/// and friends `.await` reqwest HTTP when they hit the network; GPUI's
/// executor is not tokio, so we hop runtimes here rather than inside
/// each view.
static TOKIO: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();

fn tokio() -> &'static tokio::runtime::Runtime {
    TOKIO.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("skillstar-domain")
            .build()
            .expect("tokio runtime")
    })
}

/// A domain task that panicked or was cancelled before producing a value.
#[derive(Debug, Clone)]
pub struct DomainPanic(String);

impl std::fmt::Display for DomainPanic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Background task failed: {}", self.0)
    }
}

impl std::error::Error for DomainPanic {}

/// Error types a panicked domain task can be reported as.
pub trait PanicError {
    fn from_panic(panic: DomainPanic) -> Self;
}

impl PanicError for String {
    fn from_panic(panic: DomainPanic) -> Self {
        panic.to_string()
    }
}

impl PanicError for anyhow::Error {
    fn from_panic(panic: DomainPanic) -> Self {
        anyhow::Error::new(panic)
    }
}

impl PanicError for ss_core::infra::error::AppError {
    fn from_panic(panic: DomainPanic) -> Self {
        Self::Other(panic.to_string())
    }
}

/// What a view receives when its domain task died instead of returning.
/// Views clear their busy state in `update`, so a panic must still call it;
/// outputs without an error channel fall back to their empty value.
pub trait DomainOutput: Sized {
    fn from_panic(panic: DomainPanic) -> Self;
}

impl<T, E: PanicError> DomainOutput for Result<T, E> {
    fn from_panic(panic: DomainPanic) -> Self {
        Err(E::from_panic(panic))
    }
}

impl<T: Default> DomainOutput for Result<T, tokio::task::JoinError> {
    fn from_panic(_panic: DomainPanic) -> Self {
        Ok(T::default())
    }
}

macro_rules! empty_domain_output {
    ($($ty:ty),* $(,)?) => {
        $(impl DomainOutput for $ty {
            fn from_panic(_panic: DomainPanic) -> Self {
                Default::default()
            }
        })*
    };
}

empty_domain_output!((), bool, u64, usize);

impl<T> DomainOutput for Vec<T> {
    fn from_panic(_panic: DomainPanic) -> Self {
        Vec::new()
    }
}

impl<T> DomainOutput for Option<T> {
    fn from_panic(_panic: DomainPanic) -> Self {
        None
    }
}

impl<A: DomainOutput, B: DomainOutput> DomainOutput for (A, B) {
    fn from_panic(panic: DomainPanic) -> Self {
        (A::from_panic(panic.clone()), B::from_panic(panic))
    }
}

impl<A: DomainOutput, B: DomainOutput, C: DomainOutput, D: DomainOutput> DomainOutput
    for (A, B, C, D)
{
    fn from_panic(panic: DomainPanic) -> Self {
        (
            A::from_panic(panic.clone()),
            B::from_panic(panic.clone()),
            C::from_panic(panic.clone()),
            D::from_panic(panic),
        )
    }
}

fn join_error_message(error: tokio::task::JoinError) -> String {
    if error.is_cancelled() {
        return "cancelled".to_string();
    }
    let payload = error.into_panic();
    payload
        .downcast_ref::<&str>()
        .map(|text| text.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panicked".to_string())
}

/// Bridge: run `fut` on the domain tokio runtime, deliver its output to
/// `view` via `update` on the GPUI app context. Every domain call goes
/// through this — keeps the React-era "invoke → setState" shape without
/// needing a per-call site to know about the runtime hop. A panicking task
/// is delivered as [`DomainOutput::from_panic`].
pub fn spawn_domain<V, F, U>(view: &Entity<V>, cx: &mut Context<V>, fut: F, update: U)
where
    V: 'static,
    F: std::future::Future + Send + 'static,
    F::Output: DomainOutput + Send + 'static,
    U: FnOnce(&mut V, &mut Context<V>, F::Output) + 'static,
{
    let handle = tokio().handle().clone();
    let view = view.downgrade();
    cx.spawn(async move |_self_weak, cx| {
        let out = handle.spawn(fut).await.unwrap_or_else(|error| {
            let message = join_error_message(error);
            tracing::error!(target: "gpui", %message, "domain task did not complete");
            F::Output::from_panic(DomainPanic(message))
        });
        let _ = view.update(cx, |this, cx| {
            update(this, cx, out);
            cx.notify();
        });
    })
    .detach();
}

/// Owns the process once invoked — returns only when the GPUI app exits.
/// Called by the `skillstar` binary and the standalone `ss-gpui` bin.
pub fn run() -> anyhow::Result<()> {
    ss_core::infra::logging::init();
    ss_app::bootstrap::prepare_process();
    ss_app::bootstrap::spawn_gui_background(tokio().handle());

    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        gpui_kit::init(cx);
        install_fonts(cx);
        install_scrollbar_theme(cx);
        #[cfg(target_os = "macos")]
        install_dock_icon();
        #[cfg(target_os = "macos")]
        macos_cursor::install();
        i18n::install(cx);
        theme::install(cx);
        cx.spawn(async move |cx| {
            // Product window size. The window stays resizable; a smaller
            // display is clipped so the frame still fits.
            let window_bounds = Some(cx.update(|app| {
                let screen = app
                    .primary_display()
                    .map(|display| display.bounds().size)
                    .unwrap_or_else(|| size(px(WINDOW_W), px(WINDOW_H)));
                WindowBounds::centered(initial_window_size(screen), app)
            }));
            let options = WindowOptions {
                window_bounds,
                is_resizable: true,
                window_min_size: Some(size(px(WINDOW_MIN_W), px(WINDOW_MIN_H))),
                // Overlay title bar. The page toolbar fills this band; the
                // sidebar lane only clears the traffic lights. The app
                // drags from those regions.
                titlebar: Some(TitlebarOptions {
                    title: Some("SkillStar".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(22.0), px(18.0))),
                }),
                app_owns_titlebar_drag: true,
                ..Default::default()
            };
            let res = cx.open_window(options, |window, cx| {
                let shell = cx.new(|cx| Shell::new(window, cx));
                // Close-to-background when the user enabled it in
                // Settings → Background Run: app-level `hide()` keeps
                // the process alive (patrol continues). GPUI has no
                // per-window hide — the app-level call minimizes the
                // whole bundle, which matches the React-era semantics.
                window.on_window_should_close(cx, |_window, cx| {
                    if prefs::load().background_run {
                        cx.hide();
                        false
                    } else {
                        true
                    }
                });
                let surface = cx.new(|_| crate::shell::WindowSurface::new(shell));
                cx.new(|cx| Root::new(surface, window, cx))
            });
            if let Err(err) = res {
                tracing::error!("failed to open window: {err}");
            }
        })
        .detach();
    });
    Ok(())
}

fn initial_window_size(screen: Size<Pixels>) -> Size<Pixels> {
    size(px(WINDOW_W), px(WINDOW_H)).min(&screen)
}

#[cfg(test)]
mod tests {
    use super::initial_window_size;
    use super::layout::{WINDOW_H, WINDOW_W};
    use gpui_kit::{px, size};

    #[test]
    fn initial_window_uses_the_product_size_until_the_screen_is_smaller() {
        assert_eq!(
            initial_window_size(size(px(1920.0), px(1080.0))),
            size(px(WINDOW_W), px(WINDOW_H))
        );
        assert_eq!(
            initial_window_size(size(px(1280.0), px(700.0))),
            size(px(1280.0), px(700.0))
        );
        assert_eq!(
            initial_window_size(size(px(1600.0), px(700.0))),
            size(px(WINDOW_W), px(700.0))
        );
    }
}
