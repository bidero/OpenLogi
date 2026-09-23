//! Gesture and long-press timing for the pointer tab.
//!
//! Three per-device sliders over [`GestureTuning`]: how far a gesture swipe
//! must travel, how long a gesture button must be held before movement
//! counts, and how long a press takes to become a long press. Pure config —
//! the agent picks each committed value up through its reloaded capture
//! plans — so every slider commits once, on release.

use gpui::{
    App, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window, div, rgb,
};
use gpui_base::Button as BaseButton;
use gpui_component::{h_flex, slider::Slider, v_flex};
use openlogi_core::binding::{GestureTuning, LongPressDelay, SwipeDistance, SwipeHold};

use crate::state::{AppState, DeviceRecord, StateEvent, StateEvents};
use crate::ui::commit_slider::{CommitSlider, SliderRange};
use crate::ui::section::section_label;
use crate::ui::theme::{self, ACCENT_BLUE, Palette, Typography as _};

pub struct GesturePanel {
    distance: CommitSlider<SwipeDistance>,
    hold: CommitSlider<SwipeHold>,
    long_press: CommitSlider<LongPressDelay>,
    _state_obs: Subscription,
}

/// Replace one field of the selected device's tuning and persist the result.
fn commit(cx: &mut App, change: impl FnOnce(&mut GestureTuning)) {
    AppState::apply(cx, |state| {
        let Some(key) = state.current_record().map(DeviceRecord::device_key) else {
            return StateEvents::none();
        };
        let mut tuning = state.device_gesture_tuning(key.as_str());
        change(&mut tuning);
        state.commit_device_gesture_tuning(&key, tuning)
    });
}

impl GesturePanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let distance = CommitSlider::new(
            SliderRange::new(SwipeDistance::MIN, SwipeDistance::MAX).step(5.),
            SwipeDistance::DEFAULT,
            cx,
            |_, value, cx| commit(cx, |tuning| tuning.swipe_distance = value),
        );
        let hold = CommitSlider::new(
            SliderRange::new(SwipeHold::MIN, SwipeHold::MAX).step(10.),
            SwipeHold::DEFAULT,
            cx,
            |_, value, cx| commit(cx, |tuning| tuning.swipe_hold = value),
        );
        let long_press = CommitSlider::new(
            SliderRange::new(LongPressDelay::MIN, LongPressDelay::MAX).step(50.),
            LongPressDelay::DEFAULT,
            cx,
            |_, value, cx| commit(cx, |tuning| tuning.long_press = value),
        );
        let state_obs = AppState::repaint_on(cx, |event| {
            matches!(event, StateEvent::DeviceConfigChanged(_))
        });
        Self {
            distance,
            hold,
            long_press,
            _state_obs: state_obs,
        }
    }
}

/// One labelled slider row: title and live value, the slider, a caption.
fn slider_row(
    title: SharedString,
    value: String,
    description: SharedString,
    slider: impl IntoElement,
    pal: Palette,
) -> gpui::Div {
    v_flex()
        .gap_2()
        .child(
            h_flex()
                .justify_between()
                .items_baseline()
                .child(section_label(title, pal))
                .child(div().text_body().text_color(rgb(ACCENT_BLUE)).child(value)),
        )
        .child(slider)
        .child(
            div()
                .text_caption()
                .text_color(pal.text_muted)
                .child(description),
        )
}

fn milliseconds(value: u16) -> String {
    tr!("pointer.duration_ms", value => value.to_string()).to_string()
}

impl Render for GesturePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = theme::palette(cx);
        let committed = AppState::try_read(cx)
            .and_then(|state| {
                state
                    .current_record()
                    .map(|record| state.device_gesture_tuning(&record.config_key))
            })
            .unwrap_or_default();
        // Re-seat each thumb on a device switch or external change, never
        // mid-drag.
        self.distance.sync(committed.swipe_distance, window, cx);
        self.hold.sync(committed.swipe_hold, window, cx);
        self.long_press.sync(committed.long_press, window, cx);
        let distance = self.distance.shown(committed.swipe_distance);
        let hold = self.hold.shown(committed.swipe_hold);
        let long_press = self.long_press.shown(committed.long_press);

        let mut body = v_flex()
            .gap_4()
            .w_full()
            .child(slider_row(
                tr!("pointer.gesture_distance"),
                distance.to_string(),
                tr!("pointer.gesture_distance_description"),
                Slider::new(self.distance.slider()).horizontal(),
                pal,
            ))
            .child(slider_row(
                tr!("pointer.gesture_hold"),
                milliseconds(hold.into_inner()),
                tr!("pointer.gesture_hold_description"),
                Slider::new(self.hold.slider()).horizontal(),
                pal,
            ))
            .child(slider_row(
                tr!("pointer.long_press_delay"),
                milliseconds(long_press.into_inner()),
                tr!("pointer.long_press_delay_description"),
                Slider::new(self.long_press.slider()).horizontal(),
                pal,
            ));
        if committed != GestureTuning::default() {
            body = body.child(
                h_flex().w_full().justify_end().child(
                    BaseButton::new("gesture-tuning-reset")
                        .accessibility_label(tr!("pointer.reset_gesture_defaults"))
                        .px_2p5()
                        .py_0p5()
                        .rounded_md()
                        .border_1()
                        .border_color(pal.border)
                        .bg(pal.control)
                        .hover(|s| s.bg(pal.control_hover))
                        .focus_visible(|s| s.bg(pal.control_hover))
                        .text_caption()
                        .text_color(pal.text_muted)
                        .child(tr!("pointer.reset_gesture_defaults"))
                        .on_click(|_: &ClickEvent, _, cx| {
                            commit(cx, |tuning| *tuning = GestureTuning::default());
                        }),
                ),
            );
        }
        body
    }
}
