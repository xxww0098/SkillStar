//! Dialog entrance and in-page motion must replay the heavy view.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{
    Animation, AnimationExt as _, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, px, size,
};

use super::mount_page;

struct Counter {
    hits: Rc<Cell<usize>>,
}

impl Render for Counter {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.hits.set(self.hits.get() + 1);
        div().size_full().debug_selector(|| "cached-page".into())
    }
}

/// Same shape as the shell: the host redraws when it is notified, and the
/// page is mounted through [`mount_page`].
struct Host {
    page: gpui_kit::Entity<Counter>,
    hits: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.hits.set(self.hits.get() + 1);
        div()
            .size_full()
            .relative()
            .child(mount_page(self.page.clone().into()))
    }
}

/// Root child. Dialog frames dirty the window root; this view replays the host.
struct Surface {
    host: gpui_kit::Entity<Host>,
}

impl Render for Surface {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .child(crate::chrome::replay_view(self.host.clone().into()))
    }
}

#[gpui_kit::test]
fn dialog_entrance_does_not_rebuild_the_page(cx: &mut gpui_kit::TestAppContext) {
    crate::init_test(cx);
    cx.update(|cx| cx.set_reduce_motion(false));

    let hits = Rc::new(Cell::new(0));
    let host_hits = Rc::new(Cell::new(0));
    let hits_for_page = hits.clone();
    let hits_for_host = host_hits.clone();
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let page = cx.new(|_| Counter {
            hits: hits_for_page,
        });
        let host = cx.new(|_| Host {
            page,
            hits: hits_for_host,
        });
        let surface = cx.new(|_| Surface { host });
        Root::new(surface, window, cx)
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let mounted = hits.get();
    let host_mounted = host_hits.get();
    assert!(mounted >= 1, "the page never painted");
    assert!(host_mounted >= 1, "the shell never painted");

    cx.update(|window, cx| {
        window.open_dialog(cx, |dialog, _, _| dialog.title("Confirm").child("body"));
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let mut scheduled = 0;
    for _ in 0..5 {
        scheduled += cx.update(|window, cx| {
            let frames = window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
            frames
        });
    }
    assert!(
        scheduled > 0,
        "the dialog entrance did not ask for another frame"
    );

    let rebuilt = hits.get() - mounted;
    assert!(
        rebuilt <= 1,
        "the page rebuilt {rebuilt} times while the dialog was opening"
    );
    let host_rebuilt = host_hits.get() - host_mounted;
    assert!(
        host_rebuilt <= 1,
        "the shell rebuilt {host_rebuilt} times while the dialog was opening"
    );

    let bounds = cx
        .debug_bounds("cached-page")
        .expect("the page was not painted");
    assert!(
        bounds.size.height > px(200.),
        "the cached page collapsed: {bounds:?}"
    );
}

struct Heavy {
    hits: Rc<Cell<usize>>,
}

impl Render for Heavy {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.hits.set(self.hits.get() + 1);
        div().size_full()
    }
}

/// A spinning sibling next to a replayed body. The spin follows the display
/// link; the body does not lay out again.
struct SpinFrame {
    heavy: gpui_kit::Entity<Heavy>,
}

impl Render for SpinFrame {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .child(div().size(px(8.)).with_animation(
                "frame-tick",
                Animation::new(Duration::from_millis(400)).repeat(),
                |dot, delta| dot.opacity(0.25 + 0.75 * delta),
            ))
            .child(crate::chrome::replay_view(self.heavy.clone().into()))
    }
}

#[gpui_kit::test]
fn spin_does_not_rebuild_the_replayed_body(cx: &mut gpui_kit::TestAppContext) {
    crate::init_test(cx);
    cx.update(|cx| cx.set_reduce_motion(false));

    let hits = Rc::new(Cell::new(0));
    let hits_for_body = hits.clone();
    let (_frame, cx) = cx.add_window_view(|_window, cx| {
        let heavy = cx.new(|_| Heavy {
            hits: hits_for_body,
        });
        SpinFrame { heavy }
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let mounted = hits.get();
    assert!(mounted >= 1, "the body never painted");

    let mut scheduled = 0;
    for _ in 0..5 {
        scheduled += cx.update(|window, cx| {
            let frames = window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
            frames
        });
    }
    assert!(scheduled > 0, "the spin did not ask for another frame");

    let rebuilt = hits.get() - mounted;
    assert!(
        rebuilt <= 1,
        "the replayed body rebuilt {rebuilt} times while the sibling was spinning"
    );
}
