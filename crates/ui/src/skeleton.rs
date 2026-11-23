use crate::{ActiveTheme, StyledExt};
use gpui::{
    Animation, AnimationExt, Hsla, IntoElement, RenderOnce, StyleRefinement, Styled, bounce, div,
    ease_in_out,
};
use instant::Duration;

/// A skeleton loading placeholder element.
#[derive(IntoElement)]
pub struct Skeleton {
    style: StyleRefinement,
    secondary: bool,
}

impl Skeleton {
    /// Create a new Skeleton element.
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            secondary: false,
        }
    }

    /// Set use secondary color.
    pub fn secondary(mut self) -> Self {
        self.secondary = true;
        self
    }
}

impl Styled for Skeleton {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Skeleton {
    fn render(self, _: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        SkeletonFill {
            style: self.style,
            color: if self.secondary {
                cx.theme().skeleton.opacity(0.5)
            } else {
                cx.theme().skeleton
            },
            opacity: 1.,
        }
        .with_animation(
            "skeleton",
            Animation::new(Duration::from_secs(2))
                .repeat()
                .with_easing(bounce(ease_in_out)),
            move |mut this, delta| {
                this.opacity = 1.0 - delta * 0.5;
                this
            },
        )
        .in_own_view()
    }
}

/// The block a [`Skeleton`] pulses, kept apart so that its animation can
/// build it again each frame in a view of its own.
#[derive(IntoElement, Clone)]
struct SkeletonFill {
    style: StyleRefinement,
    color: Hsla,
    opacity: f32,
}

impl RenderOnce for SkeletonFill {
    fn render(self, _: &mut gpui::Window, _: &mut gpui::App) -> impl IntoElement {
        div()
            .w_full()
            .h_4()
            .bg(self.color)
            .refine_style(&self.style)
            .opacity(self.opacity)
    }
}
