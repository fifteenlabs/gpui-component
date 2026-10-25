use std::cell::Cell;

use gpui::{App, Bounds, FocusHandle, Global, OwnedMenu, Pixels, Point, WeakFocusHandle, Window};

/// An open popover that renders its content through `deferred`.
///
/// The focus handle is the one the popover's content tracks, so it sits in the
/// dispatch tree exactly where the content does; that is what lets one popover
/// tell whether another is nested inside it. It is held weakly because popover
/// state is element state, which is collected as soon as it stops being rendered
/// — a registration that outlived its popover would leave the application
/// believing a popover is open forever.
struct DeferredPopover {
    focus_handle: WeakFocusHandle,
    /// Written as the content is prepainted, every frame, without taking the
    /// global mutably: that would mark it changed for every view reading it.
    bounds: Cell<Bounds<Pixels>>,
}

pub(crate) fn init(cx: &mut App) {
    cx.set_global(GlobalState::new());
}

impl Global for GlobalState {}

pub struct GlobalState {
    /// Open popovers that use deferred rendering.
    /// When this list is not empty, we are inside at least one deferred context.
    /// This is used to prevent double-deferred elements which would cause GPUI to panic.
    open_deferred_popovers: Vec<DeferredPopover>,
    /// Application menus storage
    app_menus: Vec<OwnedMenu>,
}

impl GlobalState {
    pub(crate) fn new() -> Self {
        Self {
            open_deferred_popovers: Vec::new(),
            app_menus: Vec::new(),
        }
    }

    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    /// Check if we are currently inside a deferred context (e.g., inside an open Popover).
    pub(crate) fn is_in_deferred_context(&self) -> bool {
        self.open_deferred_popovers
            .iter()
            .any(|popover| popover.focus_handle.upgrade().is_some())
    }

    /// Register a popover that uses deferred rendering as open.
    ///
    /// Registering an already registered popover is a no-op, so a controlled
    /// popover may register on every render.
    pub(crate) fn register_deferred_popover(&mut self, focus_handle: &FocusHandle) {
        self.open_deferred_popovers
            .retain(|popover| popover.focus_handle.upgrade().is_some());
        if self.deferred_popover(focus_handle).is_some() {
            return;
        }
        self.open_deferred_popovers.push(DeferredPopover {
            focus_handle: focus_handle.downgrade(),
            bounds: Cell::new(Bounds::default()),
        });
    }

    /// Unregister a popover when it closes.
    pub(crate) fn unregister_deferred_popover(&mut self, focus_handle: &FocusHandle) {
        self.open_deferred_popovers
            .retain(|popover| popover.focus_handle != *focus_handle);
    }

    /// Record where an open popover's content was laid out this frame.
    ///
    /// An ancestor popover reads it to tell a click inside a nested popover from
    /// a click outside everything.
    pub(crate) fn set_deferred_popover_bounds(
        &self,
        focus_handle: &FocusHandle,
        bounds: Bounds<Pixels>,
    ) {
        if let Some(popover) = self.deferred_popover(focus_handle) {
            popover.bounds.set(bounds);
        }
    }

    /// Whether `position` lies inside the content of an open popover nested
    /// within the element tree of the popover tracking `ancestor`.
    ///
    /// Nesting is read from the dispatch tree of the last rendered frame, which
    /// is where a deferred element's nodes hang under the element that deferred
    /// them, so a popover opened from inside another popover's content counts as
    /// nested however far apart the two are drawn.
    pub(crate) fn nested_deferred_popover_contains(
        &self,
        ancestor: &FocusHandle,
        position: &Point<Pixels>,
        window: &Window,
    ) -> bool {
        self.open_deferred_popovers.iter().any(|popover| {
            popover.focus_handle != *ancestor
                && popover.bounds.get().contains(position)
                && popover
                    .focus_handle
                    .upgrade()
                    .is_some_and(|nested| ancestor.contains(&nested, window))
        })
    }

    fn deferred_popover(&self, focus_handle: &FocusHandle) -> Option<&DeferredPopover> {
        self.open_deferred_popovers
            .iter()
            .find(|popover| popover.focus_handle == *focus_handle)
    }

    /// Get the application menus
    pub fn app_menus(&self) -> &[OwnedMenu] {
        &self.app_menus
    }

    /// Set the application menus
    pub fn set_app_menus(&mut self, menus: Vec<OwnedMenu>) {
        self.app_menus = menus;
    }
}
