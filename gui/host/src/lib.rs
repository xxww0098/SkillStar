//! Wave-7 host crate: the SkillStar GUI as a single-binary nsh AppView app.
//!
//! This crate is what `src/main.rs`'s `launch_gui()` calls — same signature,
//! same process-level startup (logging → bootstrap → GUI background wakes →
//! window), nsh AppView instead of GPUI. One `run` per desktop: macOS
//! (AppKit, `nsh-platform-macos`) and Windows (Win32, `nsh-platform-windows`);
//! a platform without a host reports the gap instead of pretending.
//!
//! Shape parity with `ss_gpui::run` (crates/ss-gpui/src/lib.rs:328):
//! - `PRODUCT_VERSION` slot from the caller's `env!("CARGO_PKG_VERSION")`;
//! - `ss_core::infra::logging::init()`;
//! - `ss_app::bootstrap::prepare_process()` (migrations, legacy cleanup,
//!   marketplace snapshot wiring);
//! - `ss_app::bootstrap::spawn_gui_background` on a tokio runtime shaped
//!   like the GPUI shell's (2 workers, thread `skillstar-domain`);
//! - then the window: AppView over the embedded TSX bundle, where GPUI
//!   opened its kit window. The bundle embed is `gen/bundle.js`, produced
//!   by `nsh build -o gui/host/gen gui`;
//!   build.rs rebuilds it whenever the TSX or bridge sources are newer than
//!   the bundle (docs/errors.md, 2026-11-15), so a stale bundle never ships.
//!   run() refuses an empty stub.

#[cfg(target_os = "macos")]
use nsh_platform_macos::{AppOptions, AppSource, Appearance, Bundle, Limits, run_app};
#[cfg(target_os = "windows")]
use nsh_platform_windows::{AppOptions, AppSource, Appearance, Bundle, Limits, run_app};


/// The compiled TSX app (`nsh build`): self-contained JS against the `nsh`
/// and `nsh:rt` runtime modules the engine provides. Empty until the prep
/// step or build.rs produces it; [`run`] refuses to start on a stub.
const BUNDLE_JS: &str = include_str!("../gen/bundle.js");

/// `nsh.json` manifest: carries the `window` launch options (`titleBar` etc.)
/// that `nsh build` writes only into `gen/app.json`, not `bundle.js`.
const NSH_JSON: &str = include_str!("../../nsh.json");

/// `nsh.json` `"id"`: names where the app keeps nsh-side storage
/// (`nsh/storage`), matching the development host and `nsh test`.
const APP_ID: &str = "dev.skillstar.gui";

/// Product version as passed by the binary (`env!("CARGO_PKG_VERSION")`).
/// The TSX reads the version through the bridge crate's `ROOT_VERSION`
/// (parsed from the repository root manifest by its build.rs); this slot
/// keeps the entry-point contract identical to `ss_gpui::run` and is what
/// a future release-check facade in this host would consume.
static PRODUCT_VERSION: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// Initial window size, the GPUI shell's product size (crates/ss-gpui/
/// src/layout.rs:66-67): SHELL_CHROME_W(= 8·3+180 = 204) + PANEL_BORDER_X
/// (2) + PAGE_PAD·2 (40) + CARD_ROW_W(= 336·3+16·2 = 1040) = 1286 × 800.
const WINDOW_W: i32 = 1286;
const WINDOW_H: i32 = 800;

/// Parses `nsh.json`'s `"window"` object into [`nsh_core::desktop::MainWindowOptions`]
/// so the embedded host configures the window identically to `nsh run` (e.g.
/// hiding the native macOS title bar).
fn main_window_options() -> anyhow::Result<nsh_core::desktop::MainWindowOptions> {
    let manifest: nsh_core::JsonValue = NSH_JSON
        .parse()
        .map_err(|err| anyhow::anyhow!("invalid nsh.json: {err}"))?;
    let window = manifest
        .get("window")
        .ok_or_else(|| anyhow::anyhow!("nsh.json is missing \"window\""))?;
    nsh_core::desktop::MainWindowOptions::from_json(window)
        .map_err(|err| anyhow::anyhow!("invalid nsh.json window: {err}"))
}

/// The desktop hosts' shared process startup, the shape `ss_gpui::run` had:
/// logging, bootstrap (migrations, legacy cleanup, marketplace snapshot
/// wiring), then the domain runtime's background wakes.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn start_domain(product_version: &'static str) {
    let _ = PRODUCT_VERSION.set(product_version);
    ss_core::infra::logging::init();
    ss_app::bootstrap::prepare_process();
    ss_app::bootstrap::spawn_gui_background(tokio().handle(), product_version);
}

/// The embedded TSX app with `nsh.json`'s window options: what both desktop
/// hosts hand `run_app`. Refuses the empty stub a bare `cargo check` leaves.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn embedded_bundle(main_window: nsh_core::desktop::MainWindowOptions) -> anyhow::Result<Bundle> {
    if BUNDLE_JS.trim().is_empty() {
        anyhow::bail!(
            "the TSX bundle is not built; run: nsh build -o gui/host/gen gui"
        );
    }
    let mut bundle = Bundle::source(BUNDLE_JS.to_owned());
    bundle.app_id = Some(APP_ID.to_owned());
    bundle.main_window = Some(main_window);
    Ok(bundle)
}

/// `nsh.json` `window.title`: the app's name in the title bar and the menus.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn window_title(window: &nsh_core::desktop::MainWindowOptions) -> String {
    window
        .attributes
        .title
        .clone()
        .unwrap_or_else(|| "SkillStar".to_owned())
}

/// `nsh.json` `"singleInstance"`: nsh hands a later launch to the running app
/// instead of opening a second one, and brings its window forward (shown when
/// hidden) — Windows' way back to a window that a close press hid.
#[cfg(target_os = "windows")]
fn single_instance() -> bool {
    NSH_JSON
        .parse::<nsh_core::JsonValue>()
        .ok()
        .and_then(|manifest| manifest.get("singleInstance").and_then(|on| on.as_bool()))
        .unwrap_or(false)
}

/// Owns the process once invoked — returns only when the app quits. Same
/// contract as `ss_gpui::run(product_version)`.
#[cfg(target_os = "macos")]
pub fn run(product_version: &'static str) -> anyhow::Result<()> {
    start_domain(product_version);

    // The app's UI sets: nsh/base's components plus the SkillStar bridge
    // facade registry (the `skillstar-gui/native` module the bundle
    // imports). Same registration the generated development host does
    // (.nsh/rust/macos-host/main.rs).
    nsh_platform_macos::set_ui_sets(vec![nsh_base::registry(), set_app_ss_gui_bridge()]);
    nsh_platform_macos::set_components(app_components);

    let main_window = main_window_options()?;
    let title = window_title(&main_window);
    let bundle = embedded_bundle(main_window)?;
    install_dock_icon();
    let code = run_app(AppSource::Bundle(bundle), app_options(title));
    if code == std::process::ExitCode::SUCCESS {
        Ok(())
    } else {
        anyhow::bail!("the nsh AppView host exited with a failure code");
    }
}

/// The same app on Win32 (`nsh-platform-windows`): one window over the same
/// bundle, and the resident behavior the app asks for — nsh.json's
/// `window.closeOnPress: false` with `src/shell/residentWindow.ts` hides the
/// window on a close press, the notification area (and `"singleInstance"`
/// forwarding a later launch to this instance) brings it back. `run_app`
/// ends the process once the app quits.
#[cfg(target_os = "windows")]
pub fn run(product_version: &'static str) -> anyhow::Result<()> {
    start_domain(product_version);

    nsh_platform_windows::set_ui_sets(vec![nsh_base::registry(), set_app_ss_gui_bridge()]);
    nsh_platform_windows::set_components(app_components);

    // The same immersive window as on macOS: nsh.json hides the title bar, so
    // nsh keeps the system frame (a resize border, the caption buttons drawn
    // over the app at the top right) and the app lays its page bands out
    // around them (src/shell/caption.ts). The window's menu bar does not show
    // with a hidden title bar; the tray's "退出 SkillStar" is the visible quit.
    let main_window = main_window_options()?;
    let title = window_title(&main_window);
    let bundle = embedded_bundle(main_window)?;
    let code = run_app(AppSource::Bundle(bundle), windows_app_options(title));
    if code == std::process::ExitCode::SUCCESS {
        Ok(())
    } else {
        anyhow::bail!("the nsh AppView host exited with a failure code");
    }
}

/// The options the macOS host runs the app with: a regular Dock app.
///
/// The app hides the main window on a close press (`nsh.json`
/// `window.closeOnPress: false` with `src/shell/residentWindow.ts`), so
/// `run_app` keeps its event loop: the bridge, the domain runtime and
/// `ss-app`'s background wakes (`channel_wake`, `skill_wake`,
/// `release_check_wake`) all keep going, and the Dock icon shows the same
/// window back (AppKit's `applicationShouldHandleReopen:`, which nsh's runner
/// implements). `keep_alive_on_close` is the platform-side backstop: a close
/// that ever gets past the app hides the window instead of ending the app.
/// Quit still ends it.
#[cfg(target_os = "macos")]
fn app_options(title: String) -> AppOptions {
    AppOptions {
        app_id: APP_ID.to_owned(),
        title,
        size: (WINDOW_W, WINDOW_H),
        appearance: Appearance::Dark,
        // The product binary runs the app on its own, exactly like the
        // release runner (`nsh-macos-app`): no embedding VM limits.
        limits: Limits::standalone(),
        keep_alive_on_close: true,
        ..AppOptions::default()
    }
}

/// The options the Windows host runs the app with: the embedded icon and one
/// instance per user.
///
/// The window's close button hides it (the same app-level behavior as on
/// macOS); Windows has no Dock, so the tray entry `residentWindow.ts` sets,
/// and a second launch forwarded to this instance, are the way back.
#[cfg(target_os = "windows")]
fn windows_app_options(title: String) -> AppOptions {
    AppOptions {
        app_id: APP_ID.to_owned(),
        title,
        size: (WINDOW_W, WINDOW_H),
        appearance: Appearance::Dark,
        // Windows reads icons from a file, and a bare binary carries none of
        // its own: the window's (title bar, taskbar) and the tray's icon come
        // from the `.ico` beside this crate's `nsh.json` artwork.
        icon: app_icon_path(),
        // A later launch hands over to this one, its window forward.
        single_instance: single_instance(),
        // As on macOS: the product binary runs the app on its own.
        limits: Limits::standalone(),
        ..AppOptions::default()
    }
}

/// The app's icon as a file Windows can read: the `.ico` this crate embeds
/// (16–256 px, made from `assets/icon.png`), written once per build into the
/// temp directory. `None` when the file cannot be written: the window and
/// the tray then carry the system's default icon.
#[cfg(target_os = "windows")]
fn app_icon_path() -> Option<std::path::PathBuf> {
    const ICO: &[u8] = include_bytes!("../../assets/icon.ico");
    let dir = std::env::temp_dir().join("skillstar-gui");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("icon.ico");
    if std::fs::read(&path).ok().as_deref() != Some(ICO) {
        std::fs::write(&path, ICO).ok()?;
    }
    Some(path)
}

/// Dock & About-panel icon for the bare single-file binary (crates/ss-gpui/src/lib.rs:122 parity).
/// Without a `.app` bundle macOS falls back to the generic `exec` tile; setting
/// `applicationIconImage` after `Regular` activation policy overrides it at runtime.
#[cfg(target_os = "macos")]
fn install_dock_icon() {
    use nsh_platform_macos::{
        objc2::{AnyThread, MainThreadMarker},
        objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSImage},
        objc2_foundation::NSData,
    };

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let data = NSData::with_bytes(ss_gui_bridge::APP_ICON_PNG);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    // SAFETY: `image` is a valid decoded `NSImage` and `mtm` proves we are on the AppKit main thread.
    unsafe { app.setApplicationIconImage(Some(&image)) };
}

/// Not yet ported hosts: report instead of pretending. The nsh platform
/// crates for Linux exist upstream; wiring them is a follow-up, not a silent
/// no-op.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn run(_product_version: &'static str) -> anyhow::Result<()> {
    anyhow::bail!("ss-gui-host currently ships the macOS and Windows AppView hosts only")
}

/// The domain runtime the background wakes run on — the same shape the GPUI
/// shell builds (crates/ss-gpui/src/lib.rs:176-185): multi-thread, 2
/// workers, thread name `skillstar-domain`.
fn tokio() -> &'static tokio::runtime::Runtime {
    static TOKIO: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    TOKIO.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("skillstar-domain")
            .build()
            .expect("tokio runtime")
    })
}

/// The UI set of the crate `ss-gui-bridge`: the entry
/// `#[native_module("…")]` makes in it (the pattern of the generated host).
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn set_app_ss_gui_bridge() -> nsh_core::element::Registry {
    #[allow(unused_imports)]
    use ss_gui_bridge::*;
    __nsh_native_module() // the crate ss-gui-bridge (nsh.json "rust")
}

// Without the entry: nsh's stand-in, which fails the build saying what to add.
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[allow(unused_imports)]
use nsh_core::element::__private::no_native_module as __nsh_native_module;

/// The app's native components on this platform: what
/// `#[native_components(macos)]` registers in its crates. The bridge has none
/// yet.
#[cfg(target_os = "macos")]
fn app_components(__nsh_components: &nsh_platform_macos::Components) {
    {
        #[allow(unused_imports)]
        use ss_gui_bridge::*;
        __nsh_native_components_macos(__nsh_components); // the crate ss-gui-bridge
    }
}

// Without the entry: nsh's stand-in, which registers nothing.
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
use nsh_core::element::__private::no_native_components as __nsh_native_components_macos;

/// The same on Win32: `#[native_components(windows)]`, which the bridge does
/// not declare yet, so nsh's stand-in answers (nothing registered).
#[cfg(target_os = "windows")]
fn app_components(__nsh_components: &nsh_platform_windows::Components) {
    {
        #[allow(unused_imports)]
        use ss_gui_bridge::*;
        __nsh_native_components_windows(__nsh_components); // the crate ss-gui-bridge
    }
}

// Without the entry: nsh's stand-in, which registers nothing.
#[cfg(target_os = "windows")]
#[allow(unused_imports)]
use nsh_core::element::__private::no_native_components as __nsh_native_components_windows;

#[cfg(test)]
mod tests {
    use super::{APP_ID, BUNDLE_JS, NSH_JSON, WINDOW_H, WINDOW_W, main_window_options};

    #[cfg(target_os = "macos")]
    use super::app_options;

    /// The embedded bundle is a real build, not the build.rs stub: nsh
    /// bundles start with the runtime import (see gen/bundle.js head).
    #[test]
    fn the_embedded_bundle_is_a_real_build() {
        assert!(
            BUNDLE_JS.contains("nsh:rt"),
            "gen/bundle.js is missing or a stub; run: nsh build -o gui/host/gen gui"
        );
    }

    /// The app id matches nsh.json's, so storage keys stay stable across
    /// the development host and the embedded host.
    #[test]
    fn the_app_id_matches_nsh_json() {
        assert!(NSH_JSON.contains(&format!("\"{APP_ID}\"")));
    }

    /// The embedded host parses nsh.json's window options so the hidden 44pt
    /// titleBar reaches nsh-platform-macos and no extra system title bar renders.
    #[test]
    fn the_main_window_hides_the_system_title_bar_per_nsh_json() {
        let opts = main_window_options().expect("valid nsh.json window options");
        assert_eq!(opts.attributes.title.as_deref(), Some("SkillStar"));
        let bar = opts
            .attributes
            .title_bar
            .expect("nsh.json window.titleBar configured");
        assert_eq!(bar.style, nsh_core::desktop::TitleBarStyle::Hidden);
        assert_eq!(bar.height, Some(nsh_core::geometry::Points(44.0)));
    }

    /// The resident behavior both desktop hosts share: closing the main
    /// window must not end the app. `nsh.json` asks the app first
    /// (`closeOnPress: false`), so `src/shell/residentWindow.ts` hides the
    /// window, and nsh's single-instance forwarding is the way back on
    /// Windows. The manifest is the one place both hosts and `nsh test` read.
    #[test]
    fn the_manifest_keeps_the_app_running_when_the_window_closes() {
        let window = main_window_options().expect("valid nsh.json window options");
        assert_eq!(
            window.attributes.close_on_press,
            Some(false),
            "a close press must reach the app, which hides the window"
        );
        let manifest: nsh_core::JsonValue = NSH_JSON.parse().expect("nsh.json parses");
        assert_eq!(
            manifest.get("singleInstance").and_then(|on| on.as_bool()),
            Some(true),
            "a later launch hands over to the running app instead of opening a second one"
        );
    }

    /// Closing the window on macOS leaves the app running in the Dock: the
    /// close press hides the main window (the app's handler, with
    /// `keep_alive_on_close` behind it), so the background wakes keep running
    /// and the Dock icon shows the same window back. A regular activation
    /// policy is what puts the app in the Dock at all.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_app_keeps_running_in_the_dock_when_its_window_closes() {
        use nsh_platform_macos::ActivationPolicy;

        let options = app_options("SkillStar".to_owned());
        assert!(
            options.keep_alive_on_close,
            "a closed window must hide and keep the process running"
        );
        assert_eq!(options.activation_policy, ActivationPolicy::Regular);
        assert!(!options.background);
        assert_eq!(options.size, (WINDOW_W, WINDOW_H));
        assert_eq!(options.app_id, APP_ID);
    }

    /// The Windows host opts into what the resident window needs there: an
    /// icon file (the window's taskbar button and the tray icon) and one
    /// instance per user, so a later launch brings the hidden window forward
    /// instead of starting a second app.
    #[cfg(target_os = "windows")]
    #[test]
    fn the_windows_host_keeps_one_instance_and_an_icon() {
        let options = super::windows_app_options("SkillStar".to_owned());
        assert!(options.single_instance);
        assert!(options.icon.is_some(), "the app icon must be a readable file");
        assert_eq!(options.size, (WINDOW_W, WINDOW_H));
        assert_eq!(options.app_id, APP_ID);
    }

    /// Window geometry parity: the same arithmetic as the GPUI shell's
    /// layout.rs constants (see WINDOW_W's derivation above).
    #[test]
    fn the_window_size_matches_the_gpui_product_size() {
        assert_eq!(WINDOW_W, 8 * 3 + 180 + 2 + 20 * 2 + (336 * 3 + 16 * 2));
        assert_eq!(WINDOW_H, 800);
    }

    #[test]
    fn the_app_icon_matches_nsh_json_and_decodes() {
        assert!(NSH_JSON.contains("\"icon\": \"assets/icon.png\""));
        assert!(ss_gui_bridge::APP_ICON_PNG.starts_with(b"\x89PNG\r\n\x1a\n"));

        #[cfg(target_os = "macos")]
        {
            use nsh_platform_macos::{
                objc2::AnyThread, objc2_app_kit::NSImage, objc2_foundation::NSData,
            };
            let data = NSData::with_bytes(ss_gui_bridge::APP_ICON_PNG);
            assert!(NSImage::initWithData(NSImage::alloc(), &data).is_some());
        }
    }
}
