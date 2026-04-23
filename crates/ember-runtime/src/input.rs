use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
    Space,
    Enter,
    Escape,
    A,
    D,
    W,
    S,
}

#[derive(Debug, Default)]
pub struct InputState {
    keys_held: HashSet<Key>,
    keys_pressed: HashSet<Key>,
    keys_released: HashSet<Key>,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn key_held(&self, key: Key) -> bool {
        self.keys_held.contains(&key)
    }

    pub fn key_pressed(&self, key: Key) -> bool {
        self.keys_pressed.contains(&key)
    }

    pub fn key_released(&self, key: Key) -> bool {
        self.keys_released.contains(&key)
    }

    pub fn press(&mut self, key: Key) {
        if self.keys_held.insert(key) {
            self.keys_pressed.insert(key);
        }
        self.keys_released.remove(&key);
    }

    pub fn release(&mut self, key: Key) {
        if self.keys_held.remove(&key) {
            self.keys_released.insert(key);
        }
        self.keys_pressed.remove(&key);
    }

    pub fn end_frame(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
    }
}

#[cfg(any(feature = "native", all(target_arch = "wasm32", feature = "wasm")))]
pub fn key_from_winit(key: winit::keyboard::KeyCode) -> Option<Key> {
    use winit::keyboard::KeyCode;

    match key {
        KeyCode::ArrowLeft => Some(Key::Left),
        KeyCode::ArrowRight => Some(Key::Right),
        KeyCode::ArrowUp => Some(Key::Up),
        KeyCode::ArrowDown => Some(Key::Down),
        KeyCode::Space => Some(Key::Space),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Escape => Some(Key::Escape),
        KeyCode::KeyA => Some(Key::A),
        KeyCode::KeyD => Some(Key::D),
        KeyCode::KeyW => Some(Key::W),
        KeyCode::KeyS => Some(Key::S),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_state_tracks_press_hold_release() {
        let mut input = InputState::new();

        input.press(Key::Left);
        assert!(input.key_pressed(Key::Left));
        assert!(input.key_held(Key::Left));
        assert!(!input.key_released(Key::Left));

        input.end_frame();
        assert!(!input.key_pressed(Key::Left));
        assert!(input.key_held(Key::Left));

        input.release(Key::Left);
        assert!(!input.key_held(Key::Left));
        assert!(input.key_released(Key::Left));

        input.end_frame();
        assert!(!input.key_released(Key::Left));
    }
}
