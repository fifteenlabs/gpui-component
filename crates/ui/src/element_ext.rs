use gpui::{
    AnyElement, App, Bounds, IntoElement, ParentElement, Pixels, Styled as _, Window, canvas,
};

use crate::{Sizable, Size};

#[derive(Default)]
struct ChildElementOptions {
    ix: usize,
    size: Size,
}

#[allow(patterns_in_fns_without_body)]
pub trait ChildElement: Sizable + IntoElement {
    fn with_ix(mut self, ix: usize) -> Self;
}

/// A type-erased element that can accept a [`AnyChildElementOptions`] before being rendered.
pub struct AnyChildElement(Box<dyn FnOnce(ChildElementOptions) -> AnyElement>);

impl AnyChildElement {
    pub fn new(element: impl ChildElement + 'static) -> Self {
        Self(Box::new(|options| {
            element
                .with_ix(options.ix)
                .with_size(options.size)
                .into_any_element()
        }))
    }

    pub fn into_any(self, ix: usize, size: Size) -> AnyElement {
        (self.0)(ChildElementOptions { ix, size })
    }
}

/// A trait to extend [`gpui::Element`] with additional functionality.
pub trait ElementExt: ParentElement + Sized {
    /// Add a prepaint callback to the element.
    ///
    /// This is a helper method to get the bounds of the element after paint.
    ///
    /// The first argument is the bounds of the element in pixels.
    ///
    /// The probe is an absolutely positioned child pinned to the element's
    /// top-left corner. Without explicit insets an absolute child sits at its
    /// static position, which in a block container is below the in-flow
    /// children — the probe would then report bounds starting at the element's
    /// bottom edge rather than at the element itself.
    ///
    /// See also [`gpui::canvas`].
    fn on_prepaint<F>(self, f: F) -> Self
    where
        F: FnOnce(Bounds<Pixels>, &mut Window, &mut App) + 'static,
    {
        self.child(
            canvas(
                move |bounds, window, cx| f(bounds, window, cx),
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }
}

impl<T: ParentElement> ElementExt for T {}
