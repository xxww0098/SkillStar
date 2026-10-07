//! macOS shows a new cursor for one flash, then puts the arrow back.
//!
//! GPUI's content view is layer-backed and registers a single cursor rect the
//! size of the window. AppKit applies that rect once, then drops it, and a
//! later move inside the same rect does not apply it again. GPUI still
//! believes the style is current, so it does not invalidate a second time.
//! Remember the `NSCursor` from `addCursorRect:cursor:` and set it after the
//! event, from `cursorUpdate:`, and on the next turn of the run loop.

use objc2::AnyThread;
use objc2::encode::{Encode, EncodeArguments, EncodeReturn};
use objc2::ffi::{self, OBJC_ASSOCIATION_ASSIGN, OBJC_ASSOCIATION_RETAIN};
use objc2::rc::Retained;
#[cfg(test)]
use objc2::runtime::Method;
use objc2::runtime::{AnyClass, AnyObject, MethodImplementation, NSObject, Sel};
use objc2::{msg_send, sel};
use objc2_app_kit::{NSCursor, NSTrackingArea, NSTrackingAreaOptions};
use objc2_foundation::{NSArray, NSRect};
use std::ffi::{CString, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

static INSTALLED: AtomicBool = AtomicBool::new(false);
static CURSOR_KEY: u8 = 0;
static QUEUE_KEY: u8 = 0;
static UPDATE_KEY: u8 = 0;
static ORIGINAL_MOUSE_MOVED: AtomicPtr<()> = AtomicPtr::new(ptr::null_mut());

type MouseMovedImp = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);

pub(crate) fn install() {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(class) = AnyClass::get(c"GPUIView") else {
        tracing::warn!("GPUIView is missing; the cursor override was not installed");
        return;
    };
    // Cast fn items to fn pointers. The trait is implemented for the pointer
    // type; the item type does not coerce in a trait bound.
    install_override(
        class,
        sel!(addCursorRect:cursor:),
        add_cursor_rect as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, NSRect, *mut AnyObject),
    );
    install_override(
        class,
        sel!(cursorUpdate:),
        cursor_update as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject),
    );
    install_override(
        class,
        sel!(updateTrackingAreas),
        update_tracking_areas as unsafe extern "C-unwind" fn(*mut AnyObject, Sel),
    );
    install_override(
        class,
        sel!(ss_applyCursor),
        ss_apply_cursor as unsafe extern "C-unwind" fn(*mut AnyObject, Sel),
    );
    replace_mouse_moved(class);
}

fn install_override<F>(class: &AnyClass, sel: Sel, func: F)
where
    F: MethodImplementation<Callee = AnyObject>,
{
    let types = method_type_encoding::<F>();
    let added = unsafe {
        ffi::class_addMethod(raw_class(class), sel, func.__imp(), types.as_ptr()).as_bool()
    };
    if !added {
        tracing::warn!("failed to override GPUIView method {sel:?}");
    }
}

fn replace_mouse_moved(class: &AnyClass) {
    let sel = sel!(mouseMoved:);
    let Some(method) = class.instance_method(sel) else {
        tracing::warn!("GPUIView is missing mouseMoved:");
        return;
    };
    let types = unsafe { ffi::method_getTypeEncoding(method) };
    if types.is_null() {
        tracing::warn!("GPUIView mouseMoved: has no type encoding");
        return;
    }
    let mouse_moved = mouse_moved as MouseMovedImp;
    let Some(previous) =
        (unsafe { ffi::class_replaceMethod(raw_class(class), sel, mouse_moved.__imp(), types) })
    else {
        tracing::warn!("GPUIView mouseMoved: was added instead of replaced");
        return;
    };
    let raw: *mut () = unsafe { std::mem::transmute(previous) };
    ORIGINAL_MOUSE_MOVED.store(raw, Ordering::Release);
}

fn method_type_encoding<F>() -> CString
where
    F: MethodImplementation,
{
    let ret = &F::Return::ENCODING_RETURN;
    let mut types = format!("{ret}{}{}", <*mut AnyObject>::ENCODING, Sel::ENCODING);
    for enc in F::Arguments::ENCODINGS {
        use std::fmt::Write;
        write!(&mut types, "{enc}").unwrap();
    }
    CString::new(types).unwrap()
}

fn raw_class(class: &AnyClass) -> *mut AnyClass {
    ptr::from_ref(class).cast_mut()
}

fn remember(view: *mut AnyObject, cursor: *mut AnyObject) -> bool {
    if cursor.is_null() {
        return false;
    }
    let key = ptr::from_ref(&CURSOR_KEY).cast::<c_void>();
    unsafe {
        if ffi::objc_getAssociatedObject(view, key) == cursor {
            return false;
        }
        ffi::objc_setAssociatedObject(view, key, cursor, OBJC_ASSOCIATION_RETAIN);
    }
    true
}

fn apply_cursor(view: *mut AnyObject) {
    // `[cursor set]` can run `resetCursorRects` and re-enter. One set per turn.
    static APPLYING: AtomicBool = AtomicBool::new(false);
    if APPLYING.swap(true, Ordering::Relaxed) {
        return;
    }
    let key = ptr::from_ref(&CURSOR_KEY).cast::<c_void>();
    let cursor = unsafe { ffi::objc_getAssociatedObject(view, key) };
    if !cursor.is_null() {
        let cursor: &NSCursor = unsafe { &*cursor.cast() };
        cursor.set();
    }
    APPLYING.store(false, Ordering::Relaxed);
}

fn schedule_apply(view: *mut AnyObject) {
    let key = ptr::from_ref(&QUEUE_KEY).cast::<c_void>();
    unsafe {
        if !ffi::objc_getAssociatedObject(view, key).is_null() {
            return;
        }
        ffi::objc_setAssociatedObject(view, key, queue_marker(), OBJC_ASSOCIATION_RETAIN);
        let _: () = msg_send![
            view,
            performSelector: sel!(ss_applyCursor),
            withObject: Option::<&AnyObject>::None,
            afterDelay: 0.0_f64
        ];
    }
}

/// Process-long sentinel. `Retained<NSObject>` is not `Sync`, so it cannot
/// live in a static lock; the pointer is leaked for the process lifetime.
fn queue_marker() -> *mut AnyObject {
    static MARKER: AtomicPtr<AnyObject> = AtomicPtr::new(ptr::null_mut());
    let existing = MARKER.load(Ordering::Acquire);
    if !existing.is_null() {
        return existing;
    }
    let created = Retained::into_raw(NSObject::new()).cast();
    match MARKER.compare_exchange(
        ptr::null_mut(),
        created,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => created,
        Err(existing) => {
            drop(unsafe { Retained::<NSObject>::from_raw(created.cast()) });
            existing
        }
    }
}

fn has_cursor_tracking(view: &AnyObject) -> bool {
    let areas: Retained<NSArray<NSTrackingArea>> = unsafe { msg_send![view, trackingAreas] };
    for index in 0..areas.count() {
        if areas
            .objectAtIndex(index)
            .options()
            .contains(NSTrackingAreaOptions::CursorUpdate)
        {
            return true;
        }
    }
    false
}

fn ensure_cursor_tracking(view: *mut AnyObject) {
    let key = ptr::from_ref(&UPDATE_KEY).cast::<c_void>();
    let view_ref = unsafe { &*view };
    unsafe {
        if !ffi::objc_getAssociatedObject(view, key).is_null() || has_cursor_tracking(view_ref) {
            return;
        }
        ffi::objc_setAssociatedObject(view, key, queue_marker(), OBJC_ASSOCIATION_RETAIN);
    }
    let area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            NSRect::ZERO,
            NSTrackingAreaOptions::CursorUpdate
                | NSTrackingAreaOptions::ActiveAlways
                | NSTrackingAreaOptions::InVisibleRect
                | NSTrackingAreaOptions::EnabledDuringMouseDrag,
            Some(view_ref),
            None,
        )
    };
    unsafe {
        let _: () = msg_send![view, addTrackingArea: &*area];
        ffi::objc_setAssociatedObject(view, key, ptr::null_mut(), OBJC_ASSOCIATION_ASSIGN);
    }
}

// Raw pointers, not references: a `fn(&T)` item is higher-ranked and does not
// match `MethodImplementation`'s concrete function-pointer impl.
unsafe extern "C-unwind" fn add_cursor_rect(
    this: *mut AnyObject,
    _: Sel,
    rect: NSRect,
    cursor: *mut AnyObject,
) {
    if cursor.is_null() {
        return;
    }
    let Some(ns_view) = AnyClass::get(c"NSView") else {
        return;
    };
    let this_ref = unsafe { &*this };
    let cursor_ref: &NSCursor = unsafe { &*cursor.cast() };
    let _: () =
        unsafe { msg_send![super(this_ref, ns_view), addCursorRect: rect, cursor: cursor_ref] };
    if remember(this, cursor) {
        // `set` can re-enter this method. The pointer is already stored, so
        // the re-entry does not set or schedule again.
        apply_cursor(this);
        schedule_apply(this);
    }
}

unsafe extern "C-unwind" fn cursor_update(this: *mut AnyObject, _: Sel, event: *mut AnyObject) {
    let key = ptr::from_ref(&CURSOR_KEY).cast::<c_void>();
    if unsafe { ffi::objc_getAssociatedObject(this, key) }.is_null() {
        if let Some(ns_view) = AnyClass::get(c"NSView") {
            let this_ref = unsafe { &*this };
            let event_ref = unsafe { &*event };
            let _: () = unsafe { msg_send![super(this_ref, ns_view), cursorUpdate: event_ref] };
        }
        return;
    }
    apply_cursor(this);
}

unsafe extern "C-unwind" fn update_tracking_areas(this: *mut AnyObject, _: Sel) {
    if let Some(ns_view) = AnyClass::get(c"NSView") {
        let this_ref = unsafe { &*this };
        let _: () = unsafe { msg_send![super(this_ref, ns_view), updateTrackingAreas] };
    }
    ensure_cursor_tracking(this);
}

unsafe extern "C-unwind" fn mouse_moved(this: *mut AnyObject, sel: Sel, event: *mut AnyObject) {
    let raw = ORIGINAL_MOUSE_MOVED.load(Ordering::Acquire);
    if !raw.is_null() {
        let previous: MouseMovedImp = unsafe { std::mem::transmute(raw) };
        unsafe { previous(this, sel, event) };
    }
    apply_cursor(this);
    schedule_apply(this);
}

unsafe extern "C-unwind" fn ss_apply_cursor(this: *mut AnyObject, _: Sel) {
    let key = ptr::from_ref(&QUEUE_KEY).cast::<c_void>();
    unsafe {
        ffi::objc_setAssociatedObject(this, key, ptr::null_mut(), OBJC_ASSOCIATION_ASSIGN);
    }
    apply_cursor(this);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpui_view_keeps_a_cursor_override() {
        install();
        let class = AnyClass::get(c"GPUIView").expect("GPUIView");
        for sel in [
            sel!(ss_applyCursor),
            sel!(cursorUpdate:),
            sel!(addCursorRect:cursor:),
            sel!(updateTrackingAreas),
        ] {
            assert!(class_implements(class, sel), "missing {sel:?}");
        }
        assert!(!ORIGINAL_MOUSE_MOVED.load(Ordering::Acquire).is_null());
        let ns_view = AnyClass::get(c"NSView").expect("NSView");
        for sel in [
            sel!(addCursorRect:cursor:),
            sel!(cursorUpdate:),
            sel!(updateTrackingAreas),
        ] {
            assert_eq!(
                type_codes(&encoding(class, sel)),
                type_codes(&encoding(ns_view, sel)),
                "GPUIView {sel:?} encoding drifted from NSView"
            );
        }
    }

    fn encoding(class: &AnyClass, sel: Sel) -> String {
        let method = class.instance_method(sel).expect("method");
        let ptr = unsafe { ffi::method_getTypeEncoding(method) };
        assert!(!ptr.is_null());
        unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }

    /// Apple's encoding includes stack size and offsets (`v56@0:8`). The type
    /// codes are the part that has to match the method we override.
    fn type_codes(encoding: &str) -> String {
        encoding.chars().filter(|ch| !ch.is_ascii_digit()).collect()
    }

    fn class_implements(class: &AnyClass, sel: Sel) -> bool {
        let mut count: std::ffi::c_uint = 0;
        let list = unsafe { ffi::class_copyMethodList(class, &mut count) };
        if list.is_null() {
            return false;
        }
        let methods = unsafe { std::slice::from_raw_parts(list, count as usize) };
        let found = methods.iter().any(|method| {
            let method = unsafe { method.cast::<Method>().as_ref() };
            method.is_some_and(|method| method.name() == sel)
        });
        unsafe { ffi::free(list.cast()) };
        found
    }
}
