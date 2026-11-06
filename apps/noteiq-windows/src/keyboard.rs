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
        let key = (window_id, event.physical_key);
        let translated = match event.state {
            ElementState::Pressed => {
                let Some(text) = composed_text(&event.logical_key, event.text.as_deref()) else {
                    return EventResult::Propagate;
                };
                let text: SharedString = text.into();
                self.pressed.insert(key, text.clone());
                if event.repeat {
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
        window.dispatch_event(translated);
        EventResult::PreventDefault
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
