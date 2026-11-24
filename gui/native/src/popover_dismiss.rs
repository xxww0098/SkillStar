//! Outside-click dismissal monitor for the macOS tray quota popover.
//!
//! When SkillStar is not frontmost, the quota popover (`<Window id="quota-popover">`,
//! backed by `NshPanel`) is shown via `WindowRef::show()` so AppKit does not activate
//! the entire application and raise the main window in front of other apps. Because a
//! non-activating or unfocused `NshPanel` does not receive `windowDidResignKey:` when
//! the user clicks another app, the desktop, the Dock, or blank menu-bar space, we arm
//! AppKit global + local mouse-down monitors only while the popover is open.

#[cfg(any(target_os = "macos", test))]
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};

#[cfg(any(target_os = "macos", test))]
use tokio::sync::oneshot;

/// AppKit `NSPopUpMenuWindowLevel` (101), used to recognize native popup menus
/// without coupling pure classification tests to AppKit types.
#[cfg(any(target_os = "macos", test))]
const POPUP_MENU_WINDOW_LEVEL: isize = 101;

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq)]
struct ScreenPoint {
    x: f64,
    y: f64,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq)]
struct ScreenRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowRole {
    PopoverPanel,
    StatusBarItem,
    PopupMenu,
    Other,
}

#[cfg(any(target_os = "macos", test))]
fn rect_contains(rect: ScreenRect, point: ScreenPoint) -> bool {
    rect.width > 0.0
        && rect.height > 0.0
        && point.x >= rect.x
        && point.x <= rect.x + rect.width
        && point.y >= rect.y
        && point.y <= rect.y + rect.height
}

#[cfg(any(target_os = "macos", test))]
fn classify_window(class_name: &str, level: isize) -> WindowRole {
    if class_name == "NshPanel" {
        return WindowRole::PopoverPanel;
    }
    if class_name.contains("StatusBar") {
        return WindowRole::StatusBarItem;
    }
    if level >= POPUP_MENU_WINDOW_LEVEL || class_name.contains("Menu") {
        return WindowRole::PopupMenu;
    }
    WindowRole::Other
}

/// Decide whether a mouse-down delivered to one of our own windows should dismiss
/// the quota popover.
///
/// Primary left-clicks on our status-bar icon are ignored here so `LeftMouseUp`
/// in `nsh-platform-macos` tray handling toggles the popover cleanly instead of
/// closing on mouse-down and reopening on mouse-up. Right-click / Ctrl+click on
/// the tray icon still dismisses the popover before the tray context menu opens.
#[cfg(any(target_os = "macos", test))]
fn should_dismiss_for_local_click(
    role: Option<WindowRole>,
    primary_left_click: bool,
    inside_popover: bool,
    inside_status_bar: bool,
) -> bool {
    if inside_popover {
        return false;
    }
    match role {
        Some(WindowRole::PopoverPanel | WindowRole::PopupMenu) => false,
        Some(WindowRole::StatusBarItem) => !primary_left_click,
        Some(WindowRole::Other) => true,
        None => !(primary_left_click && inside_status_bar),
    }
}

/// Decide whether a global mouse-down outside our active windows should dismiss
/// the quota popover.
#[cfg(any(target_os = "macos", test))]
fn should_dismiss_for_global_click(
    primary_left_click: bool,
    inside_popover: bool,
    inside_status_bar: bool,
) -> bool {
    if inside_popover {
        return false;
    }
    if primary_left_click && inside_status_bar {
        return false;
    }
    true
}

#[cfg(any(target_os = "macos", test))]
struct ActiveWatch {
    generation: u64,
    tx: oneshot::Sender<bool>,
}

#[cfg(any(target_os = "macos", test))]
static NEXT_GEN: AtomicU64 = AtomicU64::new(1);

#[cfg(any(target_os = "macos", test))]
static WATCHER: OnceLock<Mutex<Option<ActiveWatch>>> = OnceLock::new();

#[cfg(any(target_os = "macos", test))]
fn watcher_slot() -> &'static Mutex<Option<ActiveWatch>> {
    WATCHER.get_or_init(|| Mutex::new(None))
}

#[cfg(any(target_os = "macos", test))]
fn arm() -> (u64, oneshot::Receiver<bool>) {
    let generation = NEXT_GEN.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = oneshot::channel();
    let prev = {
        let mut guard = watcher_slot()
            .lock()
            .expect("popover dismiss watcher lock poisoned");
        guard.replace(ActiveWatch { generation, tx })
    };
    if let Some(prev) = prev {
        let _ = prev.tx.send(false);
    }
    (generation, rx)
}

#[cfg(any(target_os = "macos", test))]
fn finish(generation: u64, dismissed: bool) -> bool {
    let taken = {
        let mut guard = watcher_slot()
            .lock()
            .expect("popover dismiss watcher lock poisoned");
        if guard
            .as_ref()
            .is_some_and(|active| active.generation == generation)
        {
            guard.take()
        } else {
            None
        }
    };
    if let Some(active) = taken {
        let _ = active.tx.send(dismissed);
        true
    } else {
        false
    }
}

#[cfg(any(target_os = "macos", test))]
fn cancel() {
    let taken = {
        let mut guard = watcher_slot()
            .lock()
            .expect("popover dismiss watcher lock poisoned");
        guard.take()
    };
    if let Some(active) = taken {
        let _ = active.tx.send(false);
    }
}

#[cfg(any(target_os = "macos", test))]
fn is_active(generation: u64) -> bool {
    let guard = watcher_slot()
        .lock()
        .expect("popover dismiss watcher lock poisoned");
    guard
        .as_ref()
        .is_some_and(|active| active.generation == generation)
}

/// Wait until a mouse-down occurs outside the tray quota popover on macOS, or
/// until cancelled by closing the popover.
///
/// Always resolves `false` off macOS or in headless environments without a
/// running AppKit application.
pub(crate) async fn wait_outside_click() -> bool {
    #[cfg(target_os = "macos")]
    {
        let (generation, rx) = arm();
        std::mem::drop(nsh_core::prelude::run_on_main_thread(move || {
            macos::install(generation);
        }));
        let dismissed = rx.await.unwrap_or(false);
        std::mem::drop(nsh_core::prelude::run_on_main_thread(move || {
            macos::uninstall_if_gen(generation);
        }));
        dismissed
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Cancel any pending outside-click watcher started by [`wait_outside_click`].
pub(crate) fn cancel_outside_click() {
    #[cfg(target_os = "macos")]
    {
        cancel();
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::cell::RefCell;
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2::{rc::Retained, runtime::AnyObject, MainThreadMarker};
    use objc2_app_kit::{
        NSApplication, NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSWindow,
    };
    use objc2_foundation::NSRect;

    use super::{
        classify_window, finish, is_active, rect_contains, should_dismiss_for_global_click,
        should_dismiss_for_local_click, ScreenPoint, ScreenRect, WindowRole,
    };

    struct ActiveMonitors {
        generation: u64,
        global: Option<Retained<AnyObject>>,
        local: Option<Retained<AnyObject>>,
    }

    thread_local! {
        static MONITORS: RefCell<Option<ActiveMonitors>> = const { RefCell::new(None) };
    }

    fn remove_monitors(monitors: ActiveMonitors) {
        if let Some(global) = monitors.global {
            // SAFETY: `global` is an opaque monitor token returned by
            // `addGlobalMonitorForEventsMatchingMask:handler:`, removed outside any
            // monitor handler callback on the AppKit main thread.
            unsafe { NSEvent::removeMonitor(&global) };
        }
        if let Some(local) = monitors.local {
            // SAFETY: `local` is an opaque monitor token returned by
            // `addLocalMonitorForEventsMatchingMask:handler:`, removed outside any
            // monitor handler callback on the AppKit main thread.
            unsafe { NSEvent::removeMonitor(&local) };
        }
    }

    fn to_screen_rect(rect: NSRect) -> ScreenRect {
        ScreenRect {
            x: rect.origin.x,
            y: rect.origin.y,
            width: rect.size.width,
            height: rect.size.height,
        }
    }

    fn is_primary_left_click(event: &NSEvent) -> bool {
        event.r#type() == NSEventType::LeftMouseDown
            && !event
                .modifierFlags()
                .contains(NSEventModifierFlags::Control)
    }

    fn classify_ns_window(window: &NSWindow) -> WindowRole {
        let class_name = window.class().name().to_str().unwrap_or("");
        classify_window(class_name, window.level())
    }

    fn window_hit_flags(app: &NSApplication, point: ScreenPoint) -> (bool, bool) {
        let mut inside_popover = false;
        let mut inside_status_bar = false;
        let windows = app.windows();
        for idx in 0..windows.len() {
            let window = windows.objectAtIndex(idx);
            if !window.isVisible() {
                continue;
            }
            match classify_ns_window(&window) {
                WindowRole::PopoverPanel => {
                    if rect_contains(to_screen_rect(window.frame()), point) {
                        inside_popover = true;
                    }
                }
                WindowRole::StatusBarItem => {
                    if rect_contains(to_screen_rect(window.frame()), point) {
                        inside_status_bar = true;
                    }
                }
                WindowRole::PopupMenu | WindowRole::Other => {}
            }
        }
        (inside_popover, inside_status_bar)
    }

    pub(super) fn install(generation: u64) {
        let Some(mtm) = MainThreadMarker::new() else {
            finish(generation, false);
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        if !app.isRunning() {
            finish(generation, false);
            return;
        }
        if !is_active(generation) {
            return;
        }

        if let Some(prev) = MONITORS.with(|slot| slot.borrow_mut().take()) {
            remove_monitors(prev);
        }

        let mask =
            NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown;

        let global_block = RcBlock::new(move |event_ptr: NonNull<NSEvent>| {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            // SAFETY: AppKit delivers a valid `NSEvent` reference for the duration
            // of the global monitor callback.
            let event = unsafe { event_ptr.as_ref() };
            let primary_left = is_primary_left_click(event);
            let loc = NSEvent::mouseLocation();
            let point = ScreenPoint { x: loc.x, y: loc.y };
            let app = NSApplication::sharedApplication(mtm);
            let (inside_popover, inside_status_bar) = window_hit_flags(&app, point);
            if should_dismiss_for_global_click(primary_left, inside_popover, inside_status_bar) {
                // Never call `NSEvent::removeMonitor` inside the handler block itself;
                // completing the channel triggers uninstall on a subsequent main-thread hop.
                finish(generation, true);
            }
        });

        let local_block = RcBlock::new(move |event_ptr: NonNull<NSEvent>| -> *mut NSEvent {
            if let Some(mtm) = MainThreadMarker::new() {
                // SAFETY: AppKit delivers a valid `NSEvent` reference for the duration
                // of the local monitor callback.
                let event = unsafe { event_ptr.as_ref() };
                let primary_left = is_primary_left_click(event);
                let role = event.window(mtm).map(|w| classify_ns_window(&w));
                let loc = NSEvent::mouseLocation();
                let point = ScreenPoint { x: loc.x, y: loc.y };
                let app = NSApplication::sharedApplication(mtm);
                let (inside_popover, inside_status_bar) = window_hit_flags(&app, point);
                if should_dismiss_for_local_click(
                    role,
                    primary_left,
                    inside_popover,
                    inside_status_bar,
                ) {
                    finish(generation, true);
                }
            }
            event_ptr.as_ptr()
        });

        let global = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global_block);
        // SAFETY: `local_block` always returns the original non-null `NSEvent` pointer
        // unchanged, and `MONITORS` is removed on the main thread outside callbacks.
        let local = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local_block) };

        if global.is_none() && local.is_none() {
            finish(generation, false);
            return;
        }

        MONITORS.with(|slot| {
            *slot.borrow_mut() = Some(ActiveMonitors {
                generation,
                global,
                local,
            });
        });
    }

    pub(super) fn uninstall_if_gen(generation: u64) {
        if MainThreadMarker::new().is_none() {
            return;
        }
        let taken = MONITORS.with(|slot| {
            let mut borrow = slot.borrow_mut();
            if borrow
                .as_ref()
                .is_some_and(|active| active.generation == generation)
            {
                borrow.take()
            } else {
                None
            }
        });
        if let Some(monitors) = taken {
            remove_monitors(monitors);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_window_matches_panel_status_menu_and_other() {
        assert_eq!(classify_window("NshPanel", 25), WindowRole::PopoverPanel);
        assert_eq!(
            classify_window("NSStatusBarWindow", 25),
            WindowRole::StatusBarItem
        );
        assert_eq!(
            classify_window("NSCarbonMenuWindow", 101),
            WindowRole::PopupMenu
        );
        assert_eq!(classify_window("NSWindow", 101), WindowRole::PopupMenu);
        assert_eq!(classify_window("NSPopupMenuWindow", 25), WindowRole::PopupMenu);
        assert_eq!(classify_window("NshWindow", 0), WindowRole::Other);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn popup_menu_window_level_matches_appkit() {
        assert_eq!(POPUP_MENU_WINDOW_LEVEL, objc2_app_kit::NSPopUpMenuWindowLevel);
    }

    #[test]
    fn rect_contains_handles_interior_edges_and_outside() {
        let rect = ScreenRect {
            x: 100.0,
            y: 200.0,
            width: 340.0,
            height: 468.0,
        };
        assert!(rect_contains(rect, ScreenPoint { x: 250.0, y: 400.0 }));
        assert!(rect_contains(rect, ScreenPoint { x: 100.0, y: 200.0 }));
        assert!(rect_contains(rect, ScreenPoint { x: 440.0, y: 668.0 }));
        assert!(!rect_contains(rect, ScreenPoint { x: 99.9, y: 400.0 }));
        assert!(!rect_contains(rect, ScreenPoint { x: 440.1, y: 400.0 }));
        assert!(!rect_contains(rect, ScreenPoint { x: 250.0, y: 199.9 }));
        assert!(!rect_contains(rect, ScreenPoint { x: 250.0, y: 668.1 }));
        assert!(!rect_contains(
            ScreenRect {
                width: 0.0,
                ..rect
            },
            ScreenPoint { x: 100.0, y: 200.0 }
        ));
    }

    #[test]
    fn local_click_rules_preserve_popover_menus_and_primary_tray_toggle() {
        // Clicks inside the popover panel or its popup context menu never dismiss.
        assert!(!should_dismiss_for_local_click(
            Some(WindowRole::PopoverPanel),
            true,
            true,
            false
        ));
        assert!(!should_dismiss_for_local_click(
            Some(WindowRole::PopupMenu),
            true,
            false,
            false
        ));
        assert!(!should_dismiss_for_local_click(
            Some(WindowRole::Other),
            true,
            true,
            false
        ));

        // StatusBar primary left-click delegates toggle to LeftMouseUp; right/ctrl-click dismisses.
        assert!(!should_dismiss_for_local_click(
            Some(WindowRole::StatusBarItem),
            true,
            false,
            true
        ));
        assert!(should_dismiss_for_local_click(
            Some(WindowRole::StatusBarItem),
            false,
            false,
            true
        ));

        // Clicking SkillStar's main window or another non-popover window dismisses.
        assert!(should_dismiss_for_local_click(
            Some(WindowRole::Other),
            true,
            false,
            false
        ));

        // Unknown window falls back to status-bar bounds exclusion only for primary left-click.
        assert!(!should_dismiss_for_local_click(None, true, false, true));
        assert!(should_dismiss_for_local_click(None, false, false, true));
        assert!(should_dismiss_for_local_click(None, true, false, false));
    }

    #[test]
    fn global_click_rules_dismiss_outside_except_popover_and_primary_tray_icon() {
        assert!(should_dismiss_for_global_click(true, false, false));
        assert!(should_dismiss_for_global_click(false, false, false));
        assert!(!should_dismiss_for_global_click(true, true, false));
        assert!(!should_dismiss_for_global_click(false, true, false));
        assert!(!should_dismiss_for_global_click(true, false, true));
        assert!(should_dismiss_for_global_click(false, false, true));
    }

    #[tokio::test]
    async fn watcher_arm_finish_cancel_and_generation_guard() {
        let (gen1, rx1) = arm();
        assert!(is_active(gen1));
        assert!(finish(gen1, true));
        assert!(!is_active(gen1));
        assert!(!finish(gen1, true), "finish is one-shot per generation");
        assert!(rx1.await.expect("gen1 result"));

        let (gen2, rx2) = arm();
        cancel();
        assert!(!is_active(gen2));
        assert!(!rx2.await.expect("cancelled watcher resolves false"));

        let (gen3, rx3) = arm();
        let (gen4, rx4) = arm();
        assert!(!rx3.await.expect("superseded generation resolves false"));
        assert!(!finish(gen3, true), "stale generation cannot finish gen4");
        assert!(is_active(gen4));
        assert!(finish(gen4, true));
        assert!(rx4.await.expect("gen4 resolves true"));
    }
}
