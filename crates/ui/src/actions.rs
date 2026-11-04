use gpui::{Action, App, KeyBinding, KeyBindingContextPredicate, actions};
use serde::Deserialize;

/// A binding whose keystrokes use `secondary-`, which only [`KeyBinding::load`]
/// resolves (to `cmd` on macOS, `ctrl` elsewhere).
pub(crate) fn secondary_binding<A: Action>(
    keystrokes: &str,
    action: A,
    context: Option<&str>,
    cx: &App,
) -> KeyBinding {
    let context = context.map(|context| KeyBindingContextPredicate::parse(context).unwrap().into());
    KeyBinding::load(keystrokes, Box::new(action), context, false, None, cx).unwrap()
}

#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = ui, no_json)]
pub struct Confirm {
    /// Is confirm with secondary.
    pub secondary: bool,
}

actions!(
    ui,
    [
        Cancel,
        SelectUp,
        SelectDown,
        SelectLeft,
        SelectRight,
        SelectFirst,
        SelectLast,
        SelectPrevColumn,
        SelectNextColumn,
        SelectPageUp,
        SelectPageDown
    ]
);
