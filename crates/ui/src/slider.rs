use std::ops::Range;
use std::time::Instant;

use crate::{ActiveTheme, AxisExt, ElementExt, StyledExt, animation::Lerp, h_flex};
use gpui::{
    Along, App, AppContext as _, Axis, Background, Bounds, Context, Corners, DefiniteLength,
    DragMoveEvent, Empty, Entity, EntityId, EventEmitter, Hsla, InteractiveElement, IntoElement,
    IsZero, MouseButton, MouseDownEvent, ParentElement as _, Pixels, Point, Render, RenderOnce,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px, relative,
};

#[derive(Clone)]
struct DragThumb((EntityId, bool));

impl Render for DragThumb {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(Clone)]
struct DragSlider(EntityId);

impl Render for DragSlider {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// Events emitted by the [`SliderState`].
pub enum SliderEvent {
    /// Emitted continuously while the slider value is being changed by the user.
    Change(SliderValue),
    /// Emitted once when the user releases the slider after a drag or click.
    Release(SliderValue),
}

/// The value of the slider, can be a single value or a range of values.
///
/// - Can from a f32 value, which will be treated as a single value.
/// - Or from a (f32, f32) tuple, which will be treated as a range of values.
///
/// The default value is `SliderValue::Single(0.0)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderValue {
    Single(f32),
    Range(f32, f32),
}

impl std::fmt::Display for SliderValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SliderValue::Single(value) => write!(f, "{}", value),
            SliderValue::Range(start, end) => write!(f, "{}..{}", start, end),
        }
    }
}

impl From<f32> for SliderValue {
    fn from(value: f32) -> Self {
        SliderValue::Single(value)
    }
}

impl From<(f32, f32)> for SliderValue {
    fn from(value: (f32, f32)) -> Self {
        SliderValue::Range(value.0, value.1)
    }
}

impl From<Range<f32>> for SliderValue {
    fn from(value: Range<f32>) -> Self {
        SliderValue::Range(value.start, value.end)
    }
}

impl Default for SliderValue {
    fn default() -> Self {
        SliderValue::Single(0.)
    }
}

impl SliderValue {
    /// Clamp the value to the given range.
    pub fn clamp(self, min: f32, max: f32) -> Self {
        match self {
            SliderValue::Single(value) => SliderValue::Single(value.clamp(min, max)),
            SliderValue::Range(start, end) => {
                SliderValue::Range(start.clamp(min, max), end.clamp(min, max))
            }
        }
    }

    /// Check if the value is a single value.
    #[inline]
    pub fn is_single(&self) -> bool {
        matches!(self, SliderValue::Single(_))
    }

    /// Check if the value is a range of values.
    #[inline]
    pub fn is_range(&self) -> bool {
        matches!(self, SliderValue::Range(_, _))
    }

    /// Get the start value.
    pub fn start(&self) -> f32 {
        match self {
            SliderValue::Single(value) => *value,
            SliderValue::Range(start, _) => *start,
        }
    }

    /// Get the end value.
    pub fn end(&self) -> f32 {
        match self {
            SliderValue::Single(value) => *value,
            SliderValue::Range(_, end) => *end,
        }
    }

    fn set_start(&mut self, value: f32) {
        if let SliderValue::Range(_, end) = self {
            *self = SliderValue::Range(value.min(*end), *end);
        } else {
            *self = SliderValue::Single(value);
        }
    }

    fn set_end(&mut self, value: f32) {
        if let SliderValue::Range(start, _) = self {
            *self = SliderValue::Range(*start, value.max(*start));
        } else {
            *self = SliderValue::Single(value);
        }
    }
}

/// The scale mode of the slider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SliderScale {
    /// Linear scale where values change uniformly across the slider range.
    /// This is the default mode.
    #[default]
    Linear,
    /// Logarithmic scale where the distance between values increases exponentially.
    ///
    /// This is useful for parameters that have a large range of values where smaller
    /// changes are more significant at lower values. Common examples include:
    ///
    /// - Volume controls (human hearing perception is logarithmic)
    /// - Frequency controls (musical notes follow a logarithmic scale)
    /// - Zoom levels
    /// - Any parameter where you want finer control at lower values
    ///
    /// # For example
    ///
    /// ```
    /// use gpui_component::slider::{SliderState, SliderScale};
    ///
    /// let slider = SliderState::new()
    ///     .min(1.0)    // Must be > 0 for logarithmic scale
    ///     .max(1000.0)
    ///     .scale(SliderScale::Logarithmic);
    /// ```
    ///
    /// - Moving the slider 1/3 of the way will yield ~10
    /// - Moving it 2/3 of the way will yield ~100
    /// - The full range covers 3 orders of magnitude evenly
    Logarithmic,
}

impl SliderScale {
    #[inline]
    pub fn is_linear(&self) -> bool {
        matches!(self, SliderScale::Linear)
    }

    #[inline]
    pub fn is_logarithmic(&self) -> bool {
        matches!(self, SliderScale::Logarithmic)
    }
}

/// State of the [`Slider`].
pub struct SliderState {
    min: f32,
    max: f32,
    step: f32,
    value: SliderValue,
    /// When is single value mode, only `end` is used, the start is always 0.0.
    percentage: Range<f32>,
    /// The bounds of the slider after rendered.
    bounds: Bounds<Pixels>,
    scale: SliderScale,
    /// Tracks whether the user is currently interacting with the slider so we
    /// only emit [`SliderEvent::Release`] after a real press/drag.
    dragging: bool,
    /// Where the drag in progress began. See [`DragAnchor`].
    anchor: Option<DragAnchor>,
    /// Tremor filtering for the pointer, when this slider has asked for it.
    /// See [`SliderState::smoothed`].
    smoothing: Option<OneEuroFilter>,
}

/// The 1€ filter, over a single scalar.
///
/// A pointer carries hand tremor, and a slider that redraws the world on every
/// change turns a shake too small to notice into a shake impossible to miss.
/// The obvious answer, a fixed low-pass, trades that jitter for lag at every
/// speed — the control goes rubbery precisely when you are moving it decisively.
///
/// The 1€ filter (Casiez, Roussel & Vogel, CHI 2012, <https://gery.casiez.net/1euro/>)
/// spends its smoothing where it is needed instead: it low-passes the signal's
/// own *speed*, then uses that to pick the cutoff for the signal. Slow movement
/// is tremor and gets smoothed hard; fast movement is intent and passes nearly
/// untouched, so precision at rest costs no responsiveness in motion.
///
/// Written out rather than taken from a crate: the published crates for this
/// either carry a linear-algebra dependency for the multidimensional case we do
/// not have, or have not been touched since 2019.
#[derive(Debug)]
struct OneEuroFilter {
    /// Previous input, previous filtered output, previous filtered speed, and
    /// when they were taken. `None` until the first sample.
    previous: Option<(f32, f32, f32, Instant)>,
}

impl OneEuroFilter {
    /// Cutoff at zero speed, in Hz. Lower is steadier and laggier.
    const MIN_CUTOFF: f32 = 1.0;
    /// How sharply the cutoff opens up with speed. Higher cuts lag when moving
    /// quickly, at the cost of letting more tremor through.
    const BETA: f32 = 0.02;
    /// Cutoff for the speed estimate itself, in Hz. Kept low so a noisy
    /// derivative cannot make the filter chatter between smoothing regimes.
    const DERIVATIVE_CUTOFF: f32 = 1.0;

    /// Tuned for a pointer measured in screen pixels. The procedure the paper
    /// recommends: set `BETA` to zero and lower `MIN_CUTOFF` until a held-still
    /// pointer stops jittering, then raise `BETA` until a moving one stops
    /// lagging.
    fn new() -> Self {
        Self { previous: None }
    }

    /// Forget the signal so far. Called when a drag begins, so a new gesture
    /// does not inherit the speed of the last one.
    fn reset(&mut self) {
        self.previous = None;
    }

    /// The smoothing weight for a cutoff frequency over an interval.
    fn alpha(cutoff: f32, dt: f32) -> f32 {
        let tau = 1.0 / (2.0 * std::f32::consts::PI * cutoff);
        1.0 / (1.0 + tau / dt)
    }

    fn filter(&mut self, value: f32, now: Instant) -> f32 {
        let Some((previous_value, previous_output, previous_speed, at)) = self.previous else {
            // Nothing to smooth against yet: the first sample is the truth.
            self.previous = Some((value, value, 0.0, now));
            return value;
        };

        let dt = now.duration_since(at).as_secs_f32();
        // Two samples in the same instant carry no new information about speed,
        // and dividing by the gap would be a division by zero.
        if dt <= 0.0 {
            return previous_output;
        }

        let speed = previous_speed.lerp(
            &((value - previous_value) / dt),
            Self::alpha(Self::DERIVATIVE_CUTOFF, dt),
        );

        // The whole idea, in one line: the faster the signal is moving, the
        // higher the cutoff, the less it is smoothed.
        let cutoff = Self::MIN_CUTOFF + Self::BETA * speed.abs();
        let output = previous_output.lerp(&value, Self::alpha(cutoff, dt));

        self.previous = Some((value, output, speed, now));
        output
    }
}

/// The frozen starting point of a drag.
///
/// A slider is normally read absolutely: take the pointer, subtract the
/// track's origin, divide by the track's length. That is only sound while the
/// track holds still — and a slider that *changes the layout it is drawn in*
/// moves its own track as you drag it. A UI zoom or a font-size control does
/// exactly that: every pointer position it reports is in units the setting has
/// just redefined, and the track it is measured against has just moved too. The
/// absolute reading then feeds its own output back in, and the value chases the
/// cursor instead of following it.
///
/// Anchoring breaks the loop. The pointer's position and the track's length are
/// captured once, in screen units, at the moment the drag begins; from then on
/// the value is the starting value plus however far the pointer has physically
/// travelled. Nothing in that chain re-reads the layout, so nothing the drag
/// changes can come back around.
///
/// For a slider that does not disturb its own layout this is the same
/// arithmetic as before — the anchor is set from the absolute reading on
/// mouse-down, and screen units and layout units stay in lockstep.
#[derive(Clone, Copy, Debug)]
struct DragAnchor {
    /// Pointer position along the axis when the drag began, in screen units.
    pointer: Pixels,
    /// The fraction of the track under the pointer at that moment.
    percentage: f32,
    /// The track's length when the drag began, in screen units. Captured
    /// because the track can be resized by the very value being dragged.
    track: Pixels,
}

impl SliderState {
    /// Create a new [`SliderState`].
    pub fn new() -> Self {
        Self {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            value: SliderValue::default(),
            percentage: (0.0..0.0),
            bounds: Bounds::default(),
            scale: SliderScale::default(),
            dragging: false,
            anchor: None,
            smoothing: None,
        }
    }

    /// Smooth hand tremor out of the pointer while dragging.
    ///
    /// Off by default, because smoothing buys steadiness with latency and most
    /// sliders have no jitter problem to spend it on. Worth turning on when a
    /// small change is expensive or highly visible — a control that resizes the
    /// interface it is drawn in, say, where a tremor too small to read as a
    /// number is large enough to redraw the screen.
    pub fn smoothed(mut self, smoothed: bool) -> Self {
        self.smoothing = smoothed.then(OneEuroFilter::new);
        self
    }

    /// Set the minimum value of the slider, default: 0.0
    pub fn min(mut self, min: f32) -> Self {
        if self.scale.is_logarithmic() {
            assert!(
                min > 0.0,
                "`min` must be greater than 0 for SliderScale::Logarithmic"
            );
            assert!(
                min < self.max,
                "`min` must be less than `max` for Logarithmic scale"
            );
        }
        self.min = min;
        self.update_thumb_pos();
        self
    }

    /// Set the maximum value of the slider, default: 100.0
    pub fn max(mut self, max: f32) -> Self {
        if self.scale.is_logarithmic() {
            assert!(
                max > self.min,
                "`max` must be greater than `min` for Logarithmic scale"
            );
        }
        self.max = max;
        self.update_thumb_pos();
        self
    }

    /// Set the step value of the slider, default: 1.0
    pub fn step(mut self, step: f32) -> Self {
        self.step = step;
        self
    }

    /// Set the scale of the slider, default: [`SliderScale::Linear`].
    pub fn scale(mut self, scale: SliderScale) -> Self {
        if scale.is_logarithmic() {
            assert!(
                self.min > 0.0,
                "`min` must be greater than 0 for Logarithmic scale"
            );
            assert!(
                self.max > self.min,
                "`max` must be greater than `min` for Logarithmic scale"
            );
        }
        self.scale = scale;
        self.update_thumb_pos();
        self
    }

    /// Set the default value of the slider, default: 0.0
    pub fn default_value(mut self, value: impl Into<SliderValue>) -> Self {
        self.value = value.into();
        self.update_thumb_pos();
        self
    }

    /// Set the value of the slider.
    pub fn set_value(
        &mut self,
        value: impl Into<SliderValue>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.value = value.into();
        self.update_thumb_pos();
        cx.notify();
    }

    /// Get the value of the slider.
    pub fn value(&self) -> SliderValue {
        self.value
    }

    /// Converts a value between 0.0 and 1.0 to a value between the minimum and maximum value,
    /// depending on the chosen scale.
    fn percentage_to_value(&self, percentage: f32) -> f32 {
        match self.scale {
            SliderScale::Linear => self.min + (self.max - self.min) * percentage,
            SliderScale::Logarithmic => {
                // when percentage is 0, this simplifies to (max/min)^0 * min = 1 * min = min
                // when percentage is 1, this simplifies to (max/min)^1 * min = (max*min)/min = max
                // we clamp just to make sure we don't have issue with floating point precision
                let base = self.max / self.min;
                (base.powf(percentage) * self.min).clamp(self.min, self.max)
            }
        }
    }

    /// Converts a value between the minimum and maximum value to a value between 0.0 and 1.0,
    /// depending on the chosen scale.
    fn value_to_percentage(&self, value: f32) -> f32 {
        match self.scale {
            SliderScale::Linear => {
                let range = self.max - self.min;
                if range <= 0.0 {
                    0.0
                } else {
                    (value - self.min) / range
                }
            }
            SliderScale::Logarithmic => {
                let base = self.max / self.min;
                (value / self.min).log(base).clamp(0.0, 1.0)
            }
        }
    }

    fn update_thumb_pos(&mut self) {
        match self.value {
            SliderValue::Single(value) => {
                let percentage = self.value_to_percentage(value.clamp(self.min, self.max));
                self.percentage = 0.0..percentage;
            }
            SliderValue::Range(start, end) => {
                let clamped_start = start.clamp(self.min, self.max);
                let clamped_end = end.clamp(self.min, self.max);
                self.percentage =
                    self.value_to_percentage(clamped_start)..self.value_to_percentage(clamped_end);
            }
        }
    }

    /// Begin a drag: read the pointer against the track as an absolute
    /// position, and remember where it started.
    ///
    /// This is the press, so it must be absolute — clicking a spot on the
    /// track has to jump there. Everything after it goes through
    /// [`Self::drag_to`], which is relative to what this captured.
    fn press_at(
        &mut self,
        axis: Axis,
        position: Point<Pixels>,
        is_start: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.bounds;
        let inner_pos = if axis.is_horizontal() {
            position.x - bounds.left()
        } else {
            bounds.bottom() - position.y
        };
        let total_size = bounds.size.along(axis);
        let percentage = inner_pos.clamp(px(0.), total_size) / total_size;

        // Screen units, so the anchor survives a layout the drag itself
        // changes. `zoom` is 1.0 for a window nobody has magnified.
        let zoom = window.zoom();
        if let Some(smoothing) = self.smoothing.as_mut() {
            smoothing.reset();
        }
        self.anchor = Some(DragAnchor {
            pointer: position.along(axis) * zoom,
            percentage,
            track: total_size * zoom,
        });

        self.apply_percentage(percentage, is_start, cx);
    }

    /// Continue a drag, by how far the pointer has physically moved since
    /// [`Self::press_at`].
    fn drag_to(
        &mut self,
        axis: Axis,
        position: Point<Pixels>,
        is_start: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // No anchor means the drag began somewhere this state never saw; fall
        // back to reading the pointer absolutely rather than doing nothing.
        let Some(anchor) = self.anchor else {
            return self.press_at(axis, position, is_start, window, cx);
        };
        if anchor.track <= px(0.) {
            return;
        }

        // Filtered in screen units, before anything derives a value from it —
        // the tremor is in the hand, so it is removed where the hand is.
        let pointer = position.along(axis) * window.zoom();
        let pointer = match self.smoothing.as_mut() {
            Some(smoothing) => px(smoothing.filter(f32::from(pointer), Instant::now())),
            None => pointer,
        };

        let travelled = pointer - anchor.pointer;
        // Vertical sliders count upward from the bottom, so the pointer moving
        // down is the value going away.
        let travelled = if axis.is_horizontal() {
            travelled
        } else {
            -travelled
        };

        let percentage = (anchor.percentage + travelled / anchor.track).clamp(0., 1.);
        self.apply_percentage(percentage, is_start, cx);
    }

    /// Snap a fraction of the track to a step, store it, and announce it.
    fn apply_percentage(&mut self, percentage: f32, is_start: bool, cx: &mut Context<Self>) {
        self.dragging = true;
        let step = self.step;

        let percentage = if is_start {
            percentage.clamp(0.0, self.percentage.end)
        } else {
            percentage.clamp(self.percentage.start, 1.0)
        };

        let value = self.percentage_to_value(percentage);
        let value = (value / step).round() * step;
        // Snap the thumb to the stepped value's position, not the raw
        // cursor position — otherwise the value lands on a step while the
        // thumb glides freely between them. For a near-continuous `step`
        // this is a no-op (the snapped position ≈ the cursor).
        let percentage = self.value_to_percentage(value);

        if is_start {
            self.percentage.start = percentage;
            self.value.set_start(value);
        } else {
            self.percentage.end = percentage;
            self.value.set_end(value);
        }
        cx.emit(SliderEvent::Change(self.value));
        cx.notify();
    }

    /// Emit [`SliderEvent::Release`] if the user was actively interacting
    /// with the slider. Called on mouse-up both inside and outside the slider.
    fn handle_release(&mut self, cx: &mut Context<Self>) {
        // Dropped whether or not a drag was in progress: a stale anchor would
        // make the next drag relative to where the *last* one started.
        self.anchor = None;
        if !self.dragging {
            return;
        }
        self.dragging = false;
        cx.emit(SliderEvent::Release(self.value));
    }
}

impl EventEmitter<SliderEvent> for SliderState {}

/// A Slider element.
#[derive(IntoElement)]
pub struct Slider {
    state: Entity<SliderState>,
    axis: Axis,
    style: StyleRefinement,
    disabled: bool,
}

impl Slider {
    /// Create a new [`Slider`] element bind to the [`SliderState`].
    pub fn new(state: &Entity<SliderState>) -> Self {
        Self {
            axis: Axis::Horizontal,
            state: state.clone(),
            style: StyleRefinement::default(),
            disabled: false,
        }
    }

    /// As a horizontal slider.
    pub fn horizontal(mut self) -> Self {
        self.axis = Axis::Horizontal;
        self
    }

    /// As a vertical slider.
    pub fn vertical(mut self) -> Self {
        self.axis = Axis::Vertical;
        self
    }

    /// Set the disabled state of the slider, default: false
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[allow(clippy::too_many_arguments)]
    fn render_thumb(
        &self,
        start: DefiniteLength,
        is_start: bool,
        bar_color: Background,
        thumb_color: Hsla,
        radius: Corners<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl gpui::IntoElement {
        let entity_id = self.state.entity_id();
        let axis = self.axis;
        let id = ("slider-thumb", is_start as u32);

        if self.disabled {
            return div().id(id);
        }

        div()
            .id(id)
            .absolute()
            .when(axis.is_horizontal(), |this| {
                this.top(px(-5.)).left(start).ml(-px(8.))
            })
            .when(axis.is_vertical(), |this| {
                this.bottom(start).left(px(-5.)).mb(-px(8.))
            })
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .corner_radii(radius)
            .bg(bar_color.opacity(0.5))
            .when(cx.theme().shadow, |this| this.shadow_md())
            .size_4()
            .p(px(1.))
            .child(
                div()
                    .flex_shrink_0()
                    .size_full()
                    .corner_radii(radius)
                    .bg(thumb_color),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_drag(DragThumb((entity_id, is_start)), |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_drag_move(window.listener_for(
                &self.state,
                move |view, e: &DragMoveEvent<DragThumb>, window, cx| match e.drag(cx) {
                    DragThumb((id, is_start)) => {
                        if *id != entity_id {
                            return;
                        }

                        view.drag_to(axis, e.event.position, *is_start, window, cx)
                    }
                },
            ))
    }
}

impl Styled for Slider {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        let axis = self.axis;
        let entity_id = self.state.entity_id();
        let state = self.state.read(cx);
        let is_range = state.value().is_range();
        let percentage = state.percentage.clone();
        let bar_start = relative(percentage.start);
        let bar_end = relative(1. - percentage.end);
        let rem_size = window.rem_size();

        let bar_color = self
            .style
            .background
            .clone()
            .and_then(|bg| bg.color())
            .unwrap_or(cx.theme().slider_bar.into());
        let thumb_color = self
            .style
            .text
            .color
            .unwrap_or_else(|| cx.theme().slider_thumb);
        let corner_radii = self.style.corner_radii.clone();
        let default_radius = px(999.);
        let mut radius = Corners {
            top_left: corner_radii
                .top_left
                .map(|v| v.to_pixels(rem_size))
                .unwrap_or(default_radius),
            top_right: corner_radii
                .top_right
                .map(|v| v.to_pixels(rem_size))
                .unwrap_or(default_radius),
            bottom_left: corner_radii
                .bottom_left
                .map(|v| v.to_pixels(rem_size))
                .unwrap_or(default_radius),
            bottom_right: corner_radii
                .bottom_right
                .map(|v| v.to_pixels(rem_size))
                .unwrap_or(default_radius),
        };
        if cx.theme().radius.is_zero() {
            radius.top_left = px(0.);
            radius.top_right = px(0.);
            radius.bottom_left = px(0.);
            radius.bottom_right = px(0.);
        }

        div()
            .id(("slider", self.state.entity_id()))
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .when(axis.is_vertical(), |this| this.h(px(120.)))
            .when(axis.is_horizontal(), |this| this.w_full())
            .refine_style(&self.style)
            .bg(cx.theme().transparent)
            .text_color(cx.theme().foreground)
            .when(!self.disabled, |this| {
                this.on_mouse_up(
                    MouseButton::Left,
                    window.listener_for(&self.state, |state, _, _, cx| {
                        state.handle_release(cx);
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    window.listener_for(&self.state, |state, _, _, cx| {
                        state.handle_release(cx);
                    }),
                )
            })
            .child(
                h_flex()
                    .id("slider-bar-container")
                    .when(!self.disabled, |this| {
                        this.on_mouse_down(
                            MouseButton::Left,
                            window.listener_for(
                                &self.state,
                                move |state, e: &MouseDownEvent, window, cx| {
                                    let mut is_start = false;
                                    if is_range {
                                        let bar_size = state.bounds.size.along(axis);
                                        let inner_pos = if axis.is_horizontal() {
                                            e.position.x - state.bounds.left()
                                        } else {
                                            state.bounds.bottom() - e.position.y
                                        };
                                        let center = ((percentage.end - percentage.start) / 2.0
                                            + percentage.start)
                                            * bar_size;
                                        is_start = inner_pos < center;
                                    }

                                    state.press_at(axis, e.position, is_start, window, cx)
                                },
                            ),
                        )
                    })
                    .when(!self.disabled && !is_range, |this| {
                        this.on_drag(DragSlider(entity_id), |drag, _, _, cx| {
                            cx.stop_propagation();
                            cx.new(|_| drag.clone())
                        })
                        .on_drag_move(window.listener_for(
                            &self.state,
                            move |view, e: &DragMoveEvent<DragSlider>, window, cx| match e.drag(cx)
                            {
                                DragSlider(id) => {
                                    if *id != entity_id {
                                        return;
                                    }

                                    view.drag_to(axis, e.event.position, false, window, cx)
                                }
                            },
                        ))
                    })
                    .when(axis.is_horizontal(), |this| {
                        this.items_center().h_6().w_full()
                    })
                    .when(axis.is_vertical(), |this| {
                        this.justify_center().w_6().h_full()
                    })
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("slider-bar")
                            .relative()
                            .when(axis.is_horizontal(), |this| this.w_full().h_1p5())
                            .when(axis.is_vertical(), |this| this.h_full().w_1p5())
                            .bg(bar_color.opacity(0.2))
                            .active(|this| this.bg(bar_color.opacity(0.4)))
                            .corner_radii(radius)
                            .child(
                                div()
                                    .absolute()
                                    .when(axis.is_horizontal(), |this| {
                                        this.h_full().left(bar_start).right(bar_end)
                                    })
                                    .when(axis.is_vertical(), |this| {
                                        this.w_full().bottom(bar_start).top(bar_end)
                                    })
                                    .bg(bar_color)
                                    .when(!cx.theme().radius.is_zero(), |this| this.rounded_full()),
                            )
                            .when(is_range, |this| {
                                this.child(self.render_thumb(
                                    relative(percentage.start),
                                    true,
                                    bar_color,
                                    thumb_color,
                                    radius,
                                    window,
                                    cx,
                                ))
                            })
                            .child(self.render_thumb(
                                relative(percentage.end),
                                false,
                                bar_color,
                                thumb_color,
                                radius,
                                window,
                                cx,
                            ))
                            .on_prepaint({
                                let state = self.state.clone();
                                move |bounds, _, cx| state.update(cx, |r, _| r.bounds = bounds)
                            }),
                    ),
            )
    }
}

#[cfg(test)]
mod one_euro_tests {
    use super::OneEuroFilter;
    use std::time::{Duration, Instant};

    /// 60Hz, the rate a pointer arrives at.
    const FRAME: Duration = Duration::from_millis(16);

    /// Feed a signal through the filter and return the last output.
    fn run(filter: &mut OneEuroFilter, samples: impl IntoIterator<Item = f32>) -> f32 {
        let mut now = Instant::now();
        let mut last = 0.0;
        for sample in samples {
            last = filter.filter(sample, now);
            now += FRAME;
        }
        last
    }

    /// The point of the whole thing: a hand held still but shaking should come
    /// out much steadier than it went in.
    #[test]
    fn tremor_around_a_held_position_is_damped() {
        let mut filter = OneEuroFilter::new();
        // ±2px of shake around 500, for a second.
        let tremor = (0..60).map(|i| 500.0 + if i % 2 == 0 { 2.0 } else { -2.0 });
        let settled = run(&mut filter, tremor);

        assert!(
            (settled - 500.0).abs() < 0.5,
            "tremor should be damped well inside its own amplitude, got {settled}"
        );
    }

    /// And the other half: deliberate movement must not be smoothed into
    /// treacle, or the filter has simply traded one complaint for another.
    #[test]
    fn deliberate_movement_keeps_up() {
        let mut filter = OneEuroFilter::new();
        // A steady 600px/s sweep — an ordinary drag.
        let sweep = (0..60).map(|i| 500.0 + i as f32 * 10.0);
        let arrived = run(&mut filter, sweep);

        let target = 500.0 + 59.0 * 10.0;
        assert!(
            (arrived - target).abs() < 25.0,
            "a moving pointer should stay within a few frames of the truth, got {arrived} for {target}"
        );
    }

    /// A drag starts where the pointer is, not where the last one ended.
    #[test]
    fn the_first_sample_of_a_gesture_passes_through() {
        let mut filter = OneEuroFilter::new();
        assert_eq!(filter.filter(500.0, Instant::now()), 500.0);

        run(&mut filter, [400.0, 300.0, 200.0]);
        filter.reset();
        assert_eq!(filter.filter(900.0, Instant::now()), 900.0);
    }
}
