// ═══════════════════════════════════════════════════════════════════
//  Dock right-click menu: per-subscription usage rows (macOS only)
// ═══════════════════════════════════════════════════════════════════
//
//  macOS surfaces a custom Dock menu only through the app delegate's
//  `applicationDockMenu:` method — neither Tauri nor muda wrap it. Rather than
//  replace Tauri's delegate (which would break its own wiring), we add that one
//  method to the *existing* delegate's class at runtime. AppKit then calls it
//  on each right-click, and we build a fresh menu from the latest usage rows.
//
//  The rows are recomputed cheaply on startup and after every usage refresh and
//  cached here; the native side only reads them when the menu is opened.

use std::sync::Mutex;

/// Cached Dock menu rows ("<account> · 剩余 47%"), most-urgent first.
static DOCK_MENU_LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Recompute the Dock menu rows from the latest stored snapshots. Cheap and
/// safe to call often; the native menu reads them lazily on right-click.
pub fn refresh() {
    let lines = skillstar_app::usage::dock_menu_lines();
    if let Ok(mut guard) = DOCK_MENU_LINES.lock() {
        *guard = lines;
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::Once;

    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Sel};
    use objc2::{MainThreadMarker, msg_send, sel};
    use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
    use objc2_foundation::NSString;

    use super::DOCK_MENU_LINES;

    static INSTALL: Once = Once::new();

    /// One informational (action-less → auto-disabled) menu row.
    fn info_item(mtm: MainThreadMarker, title: &str) -> Retained<NSMenuItem> {
        let item = NSMenuItem::new(mtm);
        item.setTitle(&NSString::from_str(title));
        item
    }

    /// `applicationDockMenu:` implementation. AppKit calls this on the main
    /// thread each time the Dock icon is right-clicked, so we build the menu
    /// fresh from the cached rows and hand back an autoreleased `NSMenu`.
    extern "C-unwind" fn application_dock_menu(
        _this: *mut AnyObject,
        _cmd: Sel,
        _sender: *mut AnyObject,
    ) -> *mut NSMenu {
        // Safety: AppKit dispatches delegate callbacks on the main thread.
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let menu = NSMenu::new(mtm);

        menu.addItem(&info_item(mtm, "用量额度"));

        let lines = DOCK_MENU_LINES
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        if lines.is_empty() {
            menu.addItem(&info_item(mtm, "  暂无用量数据"));
        } else {
            for line in &lines {
                menu.addItem(&info_item(mtm, &format!("  {line}")));
            }
        }

        Retained::autorelease_return(menu)
    }

    /// Teach the app delegate's class to answer `applicationDockMenu:` — once.
    /// Adds the method to the existing delegate class instead of swapping the
    /// delegate, leaving Tauri's own delegate object untouched.
    pub fn install() {
        INSTALL.call_once(|| {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let app = NSApplication::sharedApplication(mtm);
            let delegate: *mut AnyObject = unsafe { msg_send![&app, delegate] };
            if delegate.is_null() {
                return;
            }
            let cls: *const AnyClass = unsafe { msg_send![delegate, class] };

            let imp: extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> *mut NSMenu =
                application_dock_menu;
            // Safety: the signature matches `applicationDockMenu:`
            // (returns NSMenu*, takes self, _cmd, NSApplication*); encoding "@@:@".
            unsafe {
                objc2::ffi::class_addMethod(
                    cls.cast_mut(),
                    sel!(applicationDockMenu:),
                    std::mem::transmute::<
                        extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> *mut NSMenu,
                        objc2::runtime::Imp,
                    >(imp),
                    c"@@:@".as_ptr(),
                );
            }
        });
    }
}

/// Install the Dock menu hook (once). No-op off macOS.
pub fn install() {
    #[cfg(target_os = "macos")]
    platform::install();
}
