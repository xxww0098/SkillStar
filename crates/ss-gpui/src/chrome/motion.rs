//! Pointer feedback for clickable controls.
//!
//! Hover and press paint on the pointer transition, through GPUI's `.hover()`
//! and `.active()`. They do not run a spring. A spring calls
//! `request_animation_frame` until it settles, and that notifies the view and
//! every ancestor, so the whole window rebuilds on each display refresh.
//! The old interaction spring took about 370ms (roughly 40 frames at 120Hz).
//! Sweeping a card grid kept that rebuild running the entire time.
//!
//! Disabled controls stay at rest and never take the press target. Do not
//! also set `.hover()` / `.active()` on the same channels: those refinements
//! paint after this one.
//!
//! Spatial motion (the mode capsule, the skills-nav selection, the reset card) still uses
//! [`motion_spring`]: critically damped, settled within about 80ms.
//! `reduce_motion` snaps it, because `with_spring` does.

use gpui_kit::*;

use crate::theme::palette;

/// Critically damped. ω = 80 rad/s, so a unit step is inside
/// [`MOTION_EPSILON`] in about 80ms. Underdamped springs keep requesting
/// frames through the overshoot tail.
pub(crate) const INTERACTION_SPRING: SpringConfig = SpringConfig::new(6400.0, 160.0, 1.0);

/// Positional tolerance for [`motion_spring`]. Loose enough to cut the
/// sub-pixel tail, tight enough that a mode-capsule slide does not jump.
pub(crate) const MOTION_EPSILON: f32 = 0.02;

/// How far a pressed surface moves toward the foreground ink.
const PRESS_LAYER: f32 = 0.12;
/// Opacity multiplier while a control that animates opacity is pressed.
const PRESS_OPACITY: f32 = 0.88;

/// Channels the pointer owns. `None` leaves that channel on the element.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MotionPaint {
    pub bg: Option<Rgba>,
    pub fg: Option<Rgba>,
    pub border: Option<Rgba>,
    pub opacity: Option<f32>,
}

impl MotionPaint {
    pub(crate) fn new() -> Self {
        Self {
            bg: None,
            fg: None,
            border: None,
            opacity: None,
        }
    }

    pub(crate) fn bg(mut self, color: Rgba) -> Self {
        self.bg = Some(color);
        self
    }

    pub(crate) fn fg(mut self, color: Rgba) -> Self {
        self.fg = Some(color);
        self
    }

    pub(crate) fn border(mut self, color: Rgba) -> Self {
        self.border = Some(color);
        self
    }

    pub(crate) fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity);
        self
    }
}

/// One-shot spring for a position or progress that is not a hover color.
pub(crate) fn motion_spring<T: SpringTarget>(target: T) -> SpringAnimation<T> {
    SpringAnimation::new(INTERACTION_SPRING)
        .with_epsilon(MOTION_EPSILON)
        .to(target)
}

/// Fluent entry. `key` is unused: hit-testing uses the div's own element id.
/// It stays so existing call sites keep their stable names in one place.
pub(crate) trait InteractionSpring: Sized {
    fn interaction_spring(
        self,
        key: impl Into<SharedString>,
        interactive: bool,
        rest: MotionPaint,
        hover: MotionPaint,
    ) -> MotionDiv;
}

impl InteractionSpring for Stateful<Div> {
    fn interaction_spring(
        self,
        key: impl Into<SharedString>,
        interactive: bool,
        rest: MotionPaint,
        hover: MotionPaint,
    ) -> MotionDiv {
        spring_control(self, key, interactive, rest, hover)
    }
}

pub(crate) fn spring_control(
    element: Stateful<Div>,
    _key: impl Into<SharedString>,
    interactive: bool,
    rest: MotionPaint,
    hover: MotionPaint,
) -> MotionDiv {
    let (rest, hover) = align_channels(rest, hover);
    let pressed = pressed_from(&hover);
    MotionDiv {
        element: Some(element),
        interactive,
        rest,
        hover,
        pressed,
    }
}

/// Div whose colors follow the pointer. Style, children, and clicks still
/// chain; the hover and press refinements attach when the div is laid out.
pub(crate) struct MotionDiv {
    element: Option<Stateful<Div>>,
    interactive: bool,
    rest: MotionPaint,
    hover: MotionPaint,
    pressed: MotionPaint,
}

impl MotionDiv {
    fn face(&mut self) -> &mut Stateful<Div> {
        self.element.as_mut().expect("motion is still building")
    }
}

impl Styled for MotionDiv {
    fn style(&mut self) -> &mut StyleRefinement {
        self.face().style()
    }
}

impl InteractiveElement for MotionDiv {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.face().interactivity()
    }
}

impl StatefulInteractiveElement for MotionDiv {}

impl ParentElement for MotionDiv {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.face().extend(elements);
    }
}

impl IntoElement for MotionDiv {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MotionDiv {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let rest = self.rest;
        let hover = self.hover;
        let pressed = self.pressed;
        let (hover_layer, press_layer) = pointer_layers(self.interactive, &rest, &hover, &pressed);
        let mut element = paint_style(self.element.take().expect("motion layout runs once"), rest);
        if hover_layer {
            element = element.hover(move |style| paint_style(style, hover));
        }
        if press_layer {
            element = element.active(move |style| paint_style(style, pressed));
        }
        let mut element = element.into_any_element();
        let layout_id = element.request_layout(window, cx);
        (layout_id, element)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        element: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        element.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        element: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        element.paint(window, cx);
    }
}

/// Hover layer, then press layer. Either is omitted when it would paint the
/// same pixels, so the pointer does not redraw the window for no change.
fn pointer_layers(
    interactive: bool,
    rest: &MotionPaint,
    hover: &MotionPaint,
    pressed: &MotionPaint,
) -> (bool, bool) {
    if !interactive {
        return (false, false);
    }
    (paint_differs(rest, hover), paint_differs(hover, pressed))
}

fn paint_differs(left: &MotionPaint, right: &MotionPaint) -> bool {
    color_differs(left.bg, right.bg)
        || color_differs(left.fg, right.fg)
        || color_differs(left.border, right.border)
        || opacity_differs(left.opacity, right.opacity)
}

fn color_differs(left: Option<Rgba>, right: Option<Rgba>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            (left.r - right.r).abs() > 1e-4
                || (left.g - right.g).abs() > 1e-4
                || (left.b - right.b).abs() > 1e-4
                || (left.a - right.a).abs() > 1e-4
        }
        (None, None) => false,
        _ => true,
    }
}

fn opacity_differs(left: Option<f32>, right: Option<f32>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => (left - right).abs() > 1e-4,
        (None, None) => false,
        _ => true,
    }
}

fn align_channels(mut rest: MotionPaint, mut hover: MotionPaint) -> (MotionPaint, MotionPaint) {
    align_color(&mut rest.bg, &mut hover.bg);
    align_color(&mut rest.fg, &mut hover.fg);
    align_color(&mut rest.border, &mut hover.border);
    match (rest.opacity, hover.opacity) {
        (None, Some(hover_opacity)) => rest.opacity = Some(1.0_f32.min(hover_opacity)),
        (Some(rest_opacity), None) => hover.opacity = Some(rest_opacity),
        _ => {}
    }
    (rest, hover)
}

fn align_color(rest: &mut Option<Rgba>, hover: &mut Option<Rgba>) {
    match (*rest, *hover) {
        (None, Some(color)) => *rest = Some(fade(color)),
        (Some(color), None) => *hover = Some(color),
        _ => {}
    }
}

fn fade(color: Rgba) -> Rgba {
    Rgba { a: 0.0, ..color }
}

fn pressed_from(hover: &MotionPaint) -> MotionPaint {
    let ink = rgb(palette().fg);
    MotionPaint {
        bg: hover.bg.map(|color| state_layer(color, ink)),
        fg: hover.fg,
        border: hover.border.map(|color| state_layer(color, ink)),
        opacity: hover.opacity.map(|opacity| opacity * PRESS_OPACITY),
    }
}

fn state_layer(color: Rgba, ink: Rgba) -> Rgba {
    mix(
        color,
        Rgba {
            r: ink.r,
            g: ink.g,
            b: ink.b,
            a: color.a,
        },
        PRESS_LAYER,
    )
}

fn mix(from: Rgba, to: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    Rgba {
        r: from.r + (to.r - from.r) * t,
        g: from.g + (to.g - from.g) * t,
        b: from.b + (to.b - from.b) * t,
        a: from.a + (to.a - from.a) * t,
    }
}

fn paint_style<S: Styled>(mut element: S, paint: MotionPaint) -> S {
    if let Some(bg) = paint.bg {
        element = element.bg(bg);
    }
    if let Some(fg) = paint.fg {
        element = element.text_color(fg);
    }
    if let Some(border) = paint.border {
        element = element.border_color(border);
    }
    if let Some(opacity) = paint.opacity {
        element = element.opacity(opacity);
    }
    element
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui_kit::{Rgba, SpringState};

    use super::{
        INTERACTION_SPRING, MOTION_EPSILON, MotionPaint, align_channels, paint_differs,
        pointer_layers,
    };

    fn rgba(r: f32, g: f32, b: f32, a: f32) -> Rgba {
        Rgba { r, g, b, a }
    }

    #[test]
    fn spring_settles_inside_one_tenth_of_a_second() {
        let (omega, zeta) = INTERACTION_SPRING.canonical();
        assert!((zeta - 1.0).abs() < 1e-3, "zeta {zeta}");
        assert!(omega >= 80.0, "omega {omega}");
        let time = INTERACTION_SPRING.settle_time(
            SpringState {
                position: 0.0,
                velocity: 0.0,
            },
            1.0,
            MOTION_EPSILON,
        );
        assert!(time <= Duration::from_millis(100), "{time:?}");
    }

    #[test]
    fn disabled_and_noop_hover_do_not_take_a_layer() {
        let rest = MotionPaint::new().opacity(1.0);
        let same = MotionPaint::new().opacity(1.0);
        let pressed = MotionPaint::new().opacity(0.88);
        assert_eq!(
            pointer_layers(false, &rest, &same, &pressed),
            (false, false)
        );
        assert_eq!(pointer_layers(true, &rest, &same, &pressed), (false, true));
        assert!(!paint_differs(&rest, &same));
    }

    #[test]
    fn hover_color_takes_a_layer_and_press_is_separate() {
        let rest = MotionPaint::new().bg(rgba(0.0, 0.0, 0.0, 1.0));
        let hover = MotionPaint::new().bg(rgba(1.0, 0.0, 0.0, 1.0));
        let pressed = MotionPaint::new().bg(rgba(1.0, 1.0, 1.0, 1.0));
        assert_eq!(pointer_layers(true, &rest, &hover, &pressed), (true, true));
    }

    #[test]
    fn missing_rest_fades_in_instead_of_popping() {
        let (rest, hover) = align_channels(
            MotionPaint::new(),
            MotionPaint::new().bg(rgba(0.2, 0.4, 0.6, 1.0)),
        );
        assert_eq!(rest.bg.unwrap().a, 0.0);
        assert!((rest.bg.unwrap().r - 0.2).abs() < 1e-5);
        assert!((hover.bg.unwrap().a - 1.0).abs() < 1e-5);
    }
}
