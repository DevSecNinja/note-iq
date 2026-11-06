use slint::{
    SharedString,
    platform::WindowEvent,
    winit_030::{CustomApplicationHandler, EventResult, winit},
};
use std::collections::HashMap;
use winit::{
    event::{ElementState, WindowEvent as NativeEvent},
    keyboard::{Key, NamedKey, PhysicalKey},
    window::WindowId,
};

#[derive(Default)]
pub(crate) struct ComposedTextInput {
    pressed: HashMap<(WindowId, PhysicalKey), SharedString>,
}

// Slint 1.18 prefers the logical key over Windows' translated text. That loses
// dead-key output such as apostrophe+Space (logical Space, text "'") or "'x".
fn composed_text<'a>(logical_key: &Key, text: Option<&'a str>) -> Option<&'a str> {
    let text = text.filter(|text| !text.is_empty() && !text.chars().any(char::is_control))?;
    match logical_key {
        Key::Named(NamedKey::Space) if text != " " => Some(text),
        Key::Character(key) if text != key.as_str() => Some(text),
        _ => None,
    }
}

impl ComposedTextInput {
    fn dispatch_input(
        &mut self,
        key: (WindowId, PhysicalKey),
        state: ElementState,
        logical_key: &Key,
        text: Option<&str>,
        repeat: bool,
        dispatch: impl FnOnce(WindowEvent),
    ) -> EventResult {
        let translated = match state {
            ElementState::Pressed => {
                let Some(text) = composed_text(logical_key, text) else {
                    return EventResult::Propagate;
                };
                let text: SharedString = text.into();
                self.pressed.insert(key, text.clone());
                if repeat {
                    WindowEvent::KeyPressRepeated { text }
                } else {
                    WindowEvent::KeyPressed { text }
                }
            }
            ElementState::Released => {
                let Some(text) = self.pressed.remove(&key) else {
                    return EventResult::Propagate;
                };
                WindowEvent::KeyReleased { text }
            }
        };
        dispatch(translated);
        EventResult::PreventDefault
    }
}

impl CustomApplicationHandler for ComposedTextInput {
    fn window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: WindowId,
        _native_window: Option<&winit::window::Window>,
        window: Option<&slint::Window>,
        event: &NativeEvent,
    ) -> EventResult {
        if matches!(event, NativeEvent::Focused(false) | NativeEvent::Destroyed) {
            self.pressed.retain(|(id, _), _| *id != window_id);
        }
        let NativeEvent::KeyboardInput {
            event,
            is_synthetic: false,
            ..
        } = event
        else {
            return EventResult::Propagate;
        };
        let Some(window) = window else {
            return EventResult::Propagate;
        };
        self.dispatch_input(
            (window_id, event.physical_key),
            event.state,
            &event.logical_key,
            event.text.as_deref(),
            event.repeat,
            |translated| window.dispatch_event(translated),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    #[test]
    fn translated_presses_repeats_and_matching_releases_prevent_default_handling() {
        for (logical, code, translated) in [
            (Key::Named(NamedKey::Space), KeyCode::Space, "'"),
            (Key::Character("x".into()), KeyCode::KeyX, "'x"),
        ] {
            let mut handler = ComposedTextInput::default();
            let key = (WindowId::from(1_u64), PhysicalKey::Code(code));
            for repeat in [false, true] {
                let result = handler.dispatch_input(
                    key,
                    ElementState::Pressed,
                    &logical,
                    Some(translated),
                    repeat,
                    |event| match event {
                        WindowEvent::KeyPressed { text } if !repeat => {
                            assert_eq!(text.as_str(), translated);
                        }
                        WindowEvent::KeyPressRepeated { text } if repeat => {
                            assert_eq!(text.as_str(), translated);
                        }
                        _ => panic!("unexpected translated press"),
                    },
                );
                assert!(matches!(result, EventResult::PreventDefault));
            }
            let result = handler.dispatch_input(
                key,
                ElementState::Released,
                &logical,
                None,
                false,
                |event| match event {
                    WindowEvent::KeyReleased { text } => assert_eq!(text.as_str(), translated),
                    _ => panic!("unexpected translated release"),
                },
            );
            assert!(matches!(result, EventResult::PreventDefault));
            assert!(handler.pressed.is_empty());
            let result =
                handler.dispatch_input(key, ElementState::Released, &logical, None, false, |_| {
                    panic!("unmatched release must not dispatch")
                });
            assert!(matches!(result, EventResult::Propagate));
        }
    }

    #[test]
    fn ordinary_input_propagates_without_dispatching_or_tracking() {
        let mut handler = ComposedTextInput::default();
        let key = (WindowId::from(1_u64), PhysicalKey::Code(KeyCode::Space));
        for state in [ElementState::Pressed, ElementState::Released] {
            let result = handler.dispatch_input(
                key,
                state,
                &Key::Named(NamedKey::Space),
                Some(" "),
                false,
                |_| panic!("ordinary input must stay with Slint"),
            );
            assert!(matches!(result, EventResult::Propagate));
            assert!(handler.pressed.is_empty());
        }
    }

    #[test]
    fn keeps_literal_dead_key_output_instead_of_the_logical_key() {
        assert_eq!(
            composed_text(&Key::Named(NamedKey::Space), Some("'")),
            Some("'")
        );
        assert_eq!(
            composed_text(&Key::Named(NamedKey::Space), Some("\"")),
            Some("\"")
        );
        assert_eq!(
            composed_text(&Key::Character("x".into()), Some("'x")),
            Some("'x")
        );
        assert_eq!(
            composed_text(&Key::Character("x".into()), Some("^x")),
            Some("^x")
        );
    }

    #[test]
    fn leaves_ordinary_text_accents_shortcuts_and_dead_keys_to_slint() {
        for (key, text) in [
            (Key::Named(NamedKey::Space), Some(" ")),
            (Key::Character("a".into()), Some("a")),
            (Key::Character("é".into()), Some("é")),
            (Key::Character("s".into()), Some("\u{13}")),
            (Key::Character("s".into()), None),
            (Key::Named(NamedKey::Enter), Some("\r")),
            (Key::Named(NamedKey::Tab), Some("\t")),
            (Key::Dead(Some('\'')), None),
            (Key::Dead(Some('\'')), Some("")),
        ] {
            assert_eq!(composed_text(&key, text), None, "{key:?} {text:?}");
        }
    }
}
