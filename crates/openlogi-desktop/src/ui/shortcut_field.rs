//! A keyboard-shortcut field that can record the chord from the keyboard.
//!
//! The text input stays editable; "Record" moves focus to the field itself,
//! and the next non-modifier key press, with its modifiers, becomes the
//! chord. A lone Escape cancels recording. The owner reads the result with
//! [`ShortcutField::combo`] and commits it however it likes.

use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, Keystroke, ParentElement, Render, Styled, Window, div,
};
use gpui_component::{Selectable as _, Sizable as _, button::Button, h_flex, input::InputState};
use openlogi_core::binding::KeyCombo;

use crate::ui::components::{control_input, localize_placeholder};

pub(crate) struct ShortcutField {
    input: Entity<InputState>,
    focus: FocusHandle,
    recording: bool,
}

impl ShortcutField {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder()));
        Self {
            input,
            focus: cx.focus_handle(),
            recording: false,
        }
    }

    /// The chord currently in the field, if it parses.
    pub(crate) fn combo(&self, cx: &App) -> Option<KeyCombo> {
        self.input.read(cx).value().parse().ok()
    }

    fn start_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.recording = true;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.recording {
            return;
        }
        cx.stop_propagation();
        let keystroke = &event.keystroke;
        if keystroke.key == "escape" && !has_modifiers(keystroke) {
            self.recording = false;
        } else if let Some(combo) = combo_from_keystroke(keystroke) {
            let label = combo.rendered_label();
            self.input
                .update(cx, |input, cx| input.set_value(label, window, cx));
            self.recording = false;
        }
        cx.notify();
    }
}

fn placeholder() -> gpui::SharedString {
    tr!("action_ring.shortcut_e_g_cmd_plus_shift_plus_p")
}

fn has_modifiers(keystroke: &Keystroke) -> bool {
    let m = keystroke.modifiers;
    m.platform || m.control || m.alt || m.shift
}

/// The [`KeyCombo`] for a recorded key press, or `None` for a key the chord
/// model cannot express. GPUI names keys (`a`, `f5`, `enter`, `left`…) the
/// way [`KeyCombo`]'s parser reads them.
pub(crate) fn combo_from_keystroke(keystroke: &Keystroke) -> Option<KeyCombo> {
    let m = keystroke.modifiers;
    let mut parts = Vec::new();
    if m.platform {
        parts.push("Cmd");
    }
    if m.control {
        parts.push("Ctrl");
    }
    if m.alt {
        parts.push("Alt");
    }
    if m.shift {
        parts.push("Shift");
    }
    parts.push(keystroke.key.as_str());
    parts.join("+").parse().ok()
}

impl Render for ShortcutField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        localize_placeholder(&self.input, placeholder(), window, cx);
        if self.recording && !self.focus.is_focused(window) {
            // Focus moved elsewhere (a click outside): stop listening.
            self.recording = false;
        }
        let label = if self.recording {
            tr!("actions.press_keys")
        } else {
            tr!("actions.record_shortcut")
        };
        h_flex()
            .gap_2()
            .flex_1()
            .min_w_0()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(control_input(&self.input).cleanable(true)),
            )
            .child(
                Button::new("shortcut-record")
                    .compact()
                    .small()
                    .selected(self.recording)
                    .label(label)
                    .on_click(cx.listener(|this, _, window, cx| this.start_recording(window, cx))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(key: &str) -> Option<String> {
        combo_from_keystroke(&Keystroke::parse(key).expect("keystroke"))
            .map(|combo| combo.rendered_label())
    }

    #[test]
    fn a_recorded_press_becomes_the_matching_chord() {
        assert_eq!(combo("cmd-shift-4").as_deref(), Some("Cmd+Shift+4"));
        assert_eq!(combo("ctrl-alt-left").as_deref(), Some("Ctrl+Alt+Left"));
        assert_eq!(combo("f5").as_deref(), Some("F5"));
        assert_eq!(combo("cmd-space").as_deref(), Some("Cmd+Space"));
    }

    #[test]
    fn a_key_the_chord_model_cannot_express_is_ignored() {
        assert_eq!(combo("cmd-f30"), None);
    }
}
