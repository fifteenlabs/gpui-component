use crate::{Icon, IconName, Sizable, Size};
use gpui::{
    Animation, AnimationExt as _, App, Hsla, IntoElement, ParentElement, RenderOnce, Styled as _,
    Transformation, Window, div, ease_in_out, percentage, prelude::FluentBuilder as _,
};
use instant::Duration;

/// A cycling loading spinner.
#[derive(IntoElement)]
pub struct Spinner {
    size: Size,
    icon: Icon,
    speed: Duration,
    color: Option<Hsla>,
}

impl Spinner {
    /// Create a new loading spinner.
    pub fn new() -> Self {
        Self {
            size: Size::Medium,
            speed: Duration::from_secs_f64(0.8),
            icon: Icon::new(IconName::Loader),
            color: None,
        }
    }

    /// Set specified icon for the spinner.
    ///
    /// Default is [`IconName::Loader`].
    ///
    /// Please ensure the icon used is suitable for a loading spinner.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = icon.into();
        self
    }

    /// Set the icon color.
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl Sizable for Spinner {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for Spinner {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .child(
                self.icon
                    .with_size(self.size)
                    .when_some(self.color, |this, color| this.text_color(color))
                    .with_animation(
                        "circle",
                        Animation::new(self.speed).repeat().with_easing(ease_in_out),
                        |this, delta| this.transform(Transformation::rotate(percentage(delta))),
                    )
                    .in_own_view(),
            )
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::Spinner;
    use crate::{button::Button, progress::ProgressCircle, skeleton::Skeleton, theme::Theme};
    use gpui::{
        AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _,
        TestAppContext, Window, div, prelude::FluentBuilder as _, px,
    };
    use std::{cell::Cell, rc::Rc, time::Duration};

    /// A page of components shaped like a gallery: `rows` rows of a label,
    /// buttons and boxes, every `animated_every`th with a spinner, a
    /// skeleton and a loading circle.
    struct Gallery {
        rows: usize,
        animated_every: usize,
        renders: Rc<Cell<usize>>,
    }

    impl Render for Gallery {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            let animated_every = self.animated_every;
            div()
                .flex()
                .flex_col()
                .w(px(800.))
                .children((0..self.rows).map(|row| {
                    div()
                        .flex()
                        .gap_2()
                        .child(format!("Row {row}"))
                        .children((0..3).map(|ix| {
                            Button::new(("button", (row * 3 + ix) as u64)).label("Button")
                        }))
                        .children((0..6).map(|_| div().size(px(12.)).bg(gpui::black())))
                        .when(row % animated_every == 0, |this| {
                            this.child(Spinner::new())
                                .child(Skeleton::new().w(px(80.)))
                                .child(ProgressCircle::new(("loading", row as u64)).loading(true))
                        })
                }))
        }
    }

    fn measure(cx: &mut TestAppContext, rows: usize, animated_every: usize) -> usize {
        cx.update(|cx| cx.set_global(Theme::default()));
        let renders = Rc::new(Cell::new(0));
        let window = cx.add_window({
            let renders = renders.clone();
            move |_, _| Gallery {
                rows,
                animated_every,
                renders,
            }
        });
        let frame = |cx: &mut TestAppContext| {
            cx.background_executor
                .advance_clock(Duration::from_millis(16));
            cx.update_window(window.into(), |_, window, cx| {
                window.simulate_next_frame(cx);
                window.draw(cx).clear(cx);
            })
            .unwrap();
        };
        frame(cx);
        frame(cx);
        renders.set(0);
        const FRAMES: usize = 60;
        let start = std::time::Instant::now();
        for _ in 0..FRAMES {
            frame(cx);
        }
        let elapsed = start.elapsed() / FRAMES as u32;
        println!(
            "gallery of {rows} rows, {} animated: {} renders over {FRAMES} animation frames, {elapsed:?} a frame",
            rows.div_ceil(animated_every) * 3,
            renders.get()
        );
        renders.get()
    }

    /// While a gallery's animations run, its frames draw only them: the
    /// gallery itself is not rendered again.
    #[gpui::test]
    fn animations_in_a_gallery_do_not_render_it_again(cx: &mut TestAppContext) {
        assert_eq!(measure(cx, 200, 40), 0);
    }

    /// The same with an animation in every row.
    #[gpui::test]
    fn animations_in_every_row_do_not_render_the_gallery_again(cx: &mut TestAppContext) {
        assert_eq!(measure(cx, 40, 1), 0);
    }
}
