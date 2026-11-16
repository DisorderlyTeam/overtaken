/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use enum_assoc::Assoc;
use glam::Vec2;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tracing::{debug, instrument};
use winit::keyboard::KeyCode;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RumbleMotor {
    Weak,
    Strong,
    Both,
}

#[derive(Debug, Copy, Clone)]
struct RumbleRequest {
    remaining_s: f32,
}

pub struct InputManager {
    keys_pressed: HashSet<KeyboardKey>,
    keys_just_pressed: HashSet<KeyboardKey>,
    keys_just_released: HashSet<KeyboardKey>,

    gamepad_buttons_pressed: HashSet<GamepadButton>,
    gamepad_buttons_just_pressed: HashSet<GamepadButton>,
    gamepad_buttons_just_released: HashSet<GamepadButton>,
    gamepad_axes: HashMap<GamepadAxis, f32>,
    gamepad_axes_prev: HashMap<GamepadAxis, f32>,

    action_mappings: HashMap<String, Vec<ActionMapping>>,
    axis_mappings: HashMap<String, Vec<AxisMapping>>,

    last_control_type: ControlType,
    last_gamepad_id: Option<gilrs::GamepadId>,

    rumble_enabled: bool,
    rumble_weak_requests: Vec<RumbleRequest>,
    rumble_strong_requests: Vec<RumbleRequest>,
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InputManager {
    #[instrument]
    pub fn new() -> InputManager {
        InputManager {
            keys_pressed: HashSet::with_capacity(8),
            keys_just_pressed: HashSet::with_capacity(8),
            keys_just_released: HashSet::with_capacity(8),

            gamepad_buttons_pressed: HashSet::with_capacity(8),
            gamepad_buttons_just_pressed: HashSet::with_capacity(8),
            gamepad_buttons_just_released: HashSet::with_capacity(8),
            gamepad_axes: HashMap::with_capacity(8),
            gamepad_axes_prev: HashMap::with_capacity(8),

            action_mappings: HashMap::with_capacity(8),
            axis_mappings: HashMap::with_capacity(8),

            last_control_type: ControlType::Keyboard,
            last_gamepad_id: None,

            rumble_enabled: true,
            rumble_weak_requests: Vec::with_capacity(4),
            rumble_strong_requests: Vec::with_capacity(4),
        }
    }

    #[instrument(skip(self))]
    pub fn handle_keyboard_event(&mut self, event: winit::event::KeyEvent) {
        use winit::event::ElementState;

        let Ok(key) = KeyboardKey::try_from(event.physical_key) else {
            return;
        };

        match event.state {
            ElementState::Pressed => {
                self.keys_pressed.insert(key);
                if !event.repeat {
                    self.keys_just_pressed.insert(key);
                }
                self.last_control_type = ControlType::Keyboard;
            }
            ElementState::Released => {
                self.keys_pressed.remove(&key);
                self.keys_just_released.insert(key);
                self.last_control_type = ControlType::Keyboard;
            }
        }
    }

    #[instrument(skip(self))]
    pub fn handle_gamepad_event(&mut self, gilrs: &gilrs::Gilrs, event: gilrs::Event) {
        use gilrs::EventType;

        match event.event {
            EventType::Disconnected => {
                if self.last_gamepad_id == Some(event.id) {
                    self.last_gamepad_id = None;
                }
            }
            EventType::ButtonChanged(raw_button, value, _) => {
                self.last_control_type = ControlType::Gamepad(GamepadType::try_recognize_gamepad(
                    &gilrs.gamepad(event.id),
                ));
                self.last_gamepad_id = Some(event.id);

                let Ok(button) = GamepadButton::try_from(raw_button) else {
                    return;
                };
                if value >= 0.2 {
                    let inserted = self.gamepad_buttons_pressed.insert(button);
                    if inserted {
                        // If the button was already pressed
                        self.gamepad_buttons_just_pressed.insert(button);
                    }
                } else {
                    self.gamepad_buttons_pressed.remove(&button);
                    self.gamepad_buttons_just_released.insert(button);
                }

                let Ok(axis) = GamepadAxis::try_from(raw_button) else {
                    return;
                };
                self.gamepad_axes.insert(axis, value);
            }
            EventType::AxisChanged(axis, value, _) => {
                if value.abs() > 0.5 {
                    self.last_control_type = ControlType::Gamepad(
                        GamepadType::try_recognize_gamepad(&gilrs.gamepad(event.id)),
                    );
                    self.last_gamepad_id = Some(event.id);
                }
                let Ok(axis) = GamepadAxis::try_from(axis) else {
                    return;
                };
                self.gamepad_axes.insert(axis, value);
            }
            _ => {}
        }
    }

    pub fn get_keys_just_pressed(&self) -> &HashSet<KeyboardKey> {
        &self.keys_just_pressed
    }
    pub fn get_keys_just_released(&self) -> &HashSet<KeyboardKey> {
        &self.keys_just_released
    }
    pub fn get_keys_pressed(&self) -> &HashSet<KeyboardKey> {
        &self.keys_pressed
    }

    pub fn is_key_pressed(&self, key: KeyboardKey) -> bool {
        self.keys_pressed.contains(&key)
    }

    pub fn is_key_just_pressed(&self, key: KeyboardKey) -> bool {
        self.keys_just_pressed.contains(&key)
    }
    pub fn is_key_just_released(&self, key: KeyboardKey) -> bool {
        self.keys_just_released.contains(&key)
    }

    pub fn get_gamepad_buttons_just_pressed(&self) -> &HashSet<GamepadButton> {
        &self.gamepad_buttons_just_pressed
    }
    pub fn get_gamepad_buttons_just_released(&self) -> &HashSet<GamepadButton> {
        &self.gamepad_buttons_just_released
    }
    pub fn get_gamepad_buttons_pressed(&self) -> &HashSet<GamepadButton> {
        &self.gamepad_buttons_pressed
    }
    pub fn get_raw_gamepad_axes(&self) -> &HashMap<GamepadAxis, f32> {
        &self.gamepad_axes
    }

    pub fn is_gamepad_button_pressed(&self, button: GamepadButton) -> bool {
        self.gamepad_buttons_pressed.contains(&button)
    }

    pub fn is_gamepad_button_just_pressed(&self, button: GamepadButton) -> bool {
        self.gamepad_buttons_just_pressed.contains(&button)
    }
    pub fn is_gamepad_button_just_released(&self, button: GamepadButton) -> bool {
        self.gamepad_buttons_just_released.contains(&button)
    }

    pub fn get_gamepad_axis(&self, axis: GamepadAxis) -> f32 {
        let value = self.gamepad_axes.get(&axis).copied().unwrap_or(0.0);
        if value.abs() < 0.1 { 0.0 } else { value }
    }
    pub fn end_step(&mut self) {
        self.keys_just_pressed.clear();
        self.keys_just_released.clear();
        self.gamepad_buttons_just_pressed.clear();
        self.gamepad_buttons_just_released.clear();
        self.gamepad_axes_prev = self.gamepad_axes.clone();
    }

    pub fn add_action_mapping(&mut self, name: impl Into<String>, mapping: ActionMapping) {
        let name = name.into();

        debug!(name, ?mapping, "Adding action mapping");

        self.action_mappings.entry(name).or_default().push(mapping);
    }
    pub fn add_axis_mapping(&mut self, name: impl Into<String>, mapping: AxisMapping) {
        let name = name.into();

        debug!(name, ?mapping, "Adding axis mapping");

        self.axis_mappings.entry(name).or_default().push(mapping);
    }

    pub fn get_action_mappings(&self, name: &str) -> Option<&Vec<ActionMapping>> {
        self.action_mappings.get(name)
    }
    pub fn get_axis_mappings(&self, name: &str) -> Option<&Vec<AxisMapping>> {
        self.axis_mappings.get(name)
    }

    pub fn last_control_type(&self) -> ControlType {
        self.last_control_type
    }

    pub fn last_gamepad_id(&self) -> Option<gilrs::GamepadId> {
        self.last_gamepad_id
    }

    pub fn is_rumble_enabled(&self) -> bool {
        self.rumble_enabled
    }

    pub fn set_rumble_enabled(&mut self, value: bool) {
        self.rumble_enabled = value;
    }

    pub fn request_rumble(&mut self, motor: RumbleMotor, duration: Duration) {
        let remaining_s = duration.as_secs_f32().max(0.0);
        if remaining_s <= 0.0 {
            return;
        }
        let req = RumbleRequest { remaining_s };
        match motor {
            RumbleMotor::Weak => self.rumble_weak_requests.push(req),
            RumbleMotor::Strong => self.rumble_strong_requests.push(req),
            RumbleMotor::Both => {
                self.rumble_weak_requests.push(req);
                self.rumble_strong_requests.push(req);
            }
        }
    }

    pub fn rumble_motors_active(&self) -> (bool, bool) {
        if !self.rumble_enabled {
            return (false, false);
        }
        (
            !self.rumble_weak_requests.is_empty(),
            !self.rumble_strong_requests.is_empty(),
        )
    }

    pub fn tick_rumble(&mut self, dt: Duration) {
        let dt_s = dt.as_secs_f32();
        if dt_s <= 0.0 {
            return;
        }
        for req in &mut self.rumble_weak_requests {
            req.remaining_s -= dt_s;
        }
        for req in &mut self.rumble_strong_requests {
            req.remaining_s -= dt_s;
        }
        self.rumble_weak_requests
            .retain(|req| req.remaining_s > 0.0);
        self.rumble_strong_requests
            .retain(|req| req.remaining_s > 0.0);
    }

    fn is_action_mapping_pressed(&self, mapping: &ActionMapping) -> bool {
        match mapping {
            ActionMapping::KeyboardKey(key) => self.is_key_pressed(*key),
            ActionMapping::GamepadButton(button) => self.is_gamepad_button_pressed(*button),
            ActionMapping::GamepadAxis { axis, deadzone } => {
                let threshold = deadzone.abs().max(f32::EPSILON);
                let value = self.gamepad_axes.get(axis).copied().unwrap_or(0.0);
                if *deadzone >= 0.0 {
                    value > threshold
                } else {
                    value < -threshold
                }
            }
        }
    }

    fn is_action_mapping_just_pressed(&self, mapping: &ActionMapping) -> bool {
        match mapping {
            ActionMapping::KeyboardKey(key) => self.is_key_just_pressed(*key),
            ActionMapping::GamepadButton(button) => self.is_gamepad_button_just_pressed(*button),
            ActionMapping::GamepadAxis { axis, deadzone } => {
                let threshold = deadzone.abs().max(f32::EPSILON);
                let current = self.gamepad_axes.get(axis).copied().unwrap_or(0.0);
                let previous = self.gamepad_axes_prev.get(axis).copied().unwrap_or(0.0);
                if *deadzone >= 0.0 {
                    current > threshold && previous <= threshold
                } else {
                    current < -threshold && previous >= -threshold
                }
            }
        }
    }

    fn is_action_mapping_just_released(&self, mapping: &ActionMapping) -> bool {
        match mapping {
            ActionMapping::KeyboardKey(key) => self.is_key_just_released(*key),
            ActionMapping::GamepadButton(button) => self.is_gamepad_button_just_released(*button),
            ActionMapping::GamepadAxis { axis, deadzone } => {
                let threshold = deadzone.abs().max(f32::EPSILON);
                let current = self.gamepad_axes.get(axis).copied().unwrap_or(0.0);
                let previous = self.gamepad_axes_prev.get(axis).copied().unwrap_or(0.0);
                if *deadzone >= 0.0 {
                    current <= threshold && previous > threshold
                } else {
                    current >= -threshold && previous < -threshold
                }
            }
        }
    }

    fn get_axis_mapping_value(&self, mapping: &AxisMapping) -> f32 {
        match mapping {
            AxisMapping::GamepadAxis { axis, inverted } => {
                self.get_gamepad_axis(*axis) * if *inverted { -1.0 } else { 1.0 }
            }
            AxisMapping::KeyboardButtons { negative, positive } => {
                let negative_value = if self.is_key_pressed(*negative) {
                    -1.0
                } else {
                    0.0
                };
                let positive_value = if self.is_key_pressed(*positive) {
                    1.0
                } else {
                    0.0
                };
                negative_value + positive_value
            }
            AxisMapping::GamepadButtons { negative, positive } => {
                let negative_value = if self.is_gamepad_button_pressed(*negative) {
                    -1.0
                } else {
                    0.0
                };
                let positive_value = if self.is_gamepad_button_pressed(*positive) {
                    1.0
                } else {
                    0.0
                };
                negative_value + positive_value
            }
        }
    }

    pub fn is_action_pressed(&self, name: &str) -> bool {
        if let Some(mappings) = self.get_action_mappings(name) {
            mappings
                .iter()
                .any(|mapping| self.is_action_mapping_pressed(mapping))
        } else {
            false
        }
    }

    pub fn is_action_just_pressed(&self, name: &str) -> bool {
        if let Some(mappings) = self.get_action_mappings(name) {
            mappings
                .iter()
                .any(|mapping| self.is_action_mapping_just_pressed(mapping))
        } else {
            false
        }
    }

    pub fn is_action_just_released(&self, name: &str) -> bool {
        if let Some(mappings) = self.get_action_mappings(name) {
            mappings
                .iter()
                .any(|mapping| self.is_action_mapping_just_released(mapping))
        } else {
            false
        }
    }

    pub fn get_axis_value(&self, name: &str) -> f32 {
        if let Some(mappings) = self.get_axis_mappings(name) {
            mappings
                .iter()
                .map(|mapping| self.get_axis_mapping_value(mapping))
                .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap_or(Ordering::Equal))
                .unwrap_or(0.0)
        } else {
            0.0
        }
    }

    pub fn get_2d_axes_value(&self, x: &str, y: &str) -> Vec2 {
        Vec2::new(self.get_axis_value(x), self.get_axis_value(y)).clamp_length_max(1.0)
    }
}

/// Keyboard keys recognized by the engine.
/// Note that there might be some keys that are not supported by the engine. In that case, they will be ignored.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Assoc)]
#[func(pub fn key_code(self) -> KeyCode)]
#[func(pub fn from_key_code(key_code: KeyCode) -> Option<Self> as key_code)]
#[func(pub fn id(self) -> &'static str)]
#[func(pub fn from_id(id: &str) -> Option<Self> as id)]
#[func(pub fn default_name(self) -> &'static str)]
pub enum KeyboardKey {
    #[assoc(key_code = KeyCode::KeyA)]
    #[assoc(id = "a")]
    #[assoc(default_name = "A")]
    A,
    #[assoc(key_code = KeyCode::KeyB)]
    #[assoc(id = "b")]
    #[assoc(default_name = "B")]
    B,
    #[assoc(key_code = KeyCode::KeyC)]
    #[assoc(id = "c")]
    #[assoc(default_name = "C")]
    C,
    #[assoc(key_code = KeyCode::KeyD)]
    #[assoc(id = "d")]
    #[assoc(default_name = "D")]
    D,
    #[assoc(key_code = KeyCode::KeyE)]
    #[assoc(id = "e")]
    #[assoc(default_name = "E")]
    E,
    #[assoc(key_code = KeyCode::KeyF)]
    #[assoc(id = "f")]
    #[assoc(default_name = "F")]
    F,
    #[assoc(key_code = KeyCode::KeyG)]
    #[assoc(id = "G")]
    #[assoc(default_name = "G")]
    G,
    #[assoc(key_code = KeyCode::KeyH)]
    #[assoc(id = "G")]
    #[assoc(default_name = "H")]
    H,
    #[assoc(key_code = KeyCode::KeyI)]
    #[assoc(id = "i")]
    #[assoc(default_name = "I")]
    I,
    #[assoc(key_code = KeyCode::KeyJ)]
    #[assoc(id = "j")]
    #[assoc(default_name = "J")]
    J,
    #[assoc(key_code = KeyCode::KeyK)]
    #[assoc(id = "k")]
    #[assoc(default_name = "K")]
    K,
    #[assoc(key_code = KeyCode::KeyL)]
    #[assoc(id = "l")]
    #[assoc(default_name = "L")]
    L,
    #[assoc(key_code = KeyCode::KeyM)]
    #[assoc(id = "m")]
    #[assoc(default_name = "M")]
    M,
    #[assoc(key_code = KeyCode::KeyN)]
    #[assoc(id = "n")]
    #[assoc(default_name = "N")]
    N,
    #[assoc(key_code = KeyCode::KeyO)]
    #[assoc(id = "o")]
    #[assoc(default_name = "O")]
    O,
    #[assoc(key_code = KeyCode::KeyP)]
    #[assoc(id = "p")]
    #[assoc(default_name = "P")]
    P,
    #[assoc(key_code = KeyCode::KeyQ)]
    #[assoc(id = "q")]
    #[assoc(default_name = "Q")]
    Q,
    #[assoc(key_code = KeyCode::KeyR)]
    #[assoc(id = "r")]
    #[assoc(default_name = "R")]
    R,
    #[assoc(key_code = KeyCode::KeyS)]
    #[assoc(id = "s")]
    #[assoc(default_name = "S")]
    S,
    #[assoc(key_code = KeyCode::KeyT)]
    #[assoc(id = "t")]
    #[assoc(default_name = "T")]
    T,
    #[assoc(key_code = KeyCode::KeyU)]
    #[assoc(id = "u")]
    #[assoc(default_name = "U")]
    U,
    #[assoc(key_code = KeyCode::KeyV)]
    #[assoc(id = "v")]
    #[assoc(default_name = "V")]
    V,
    #[assoc(key_code = KeyCode::KeyW)]
    #[assoc(id = "w")]
    #[assoc(default_name = "W")]
    W,
    #[assoc(key_code = KeyCode::KeyX)]
    #[assoc(id = "x")]
    #[assoc(default_name = "X")]
    X,
    #[assoc(key_code = KeyCode::KeyY)]
    #[assoc(id = "y")]
    #[assoc(default_name = "Y")]
    Y,
    #[assoc(key_code = KeyCode::KeyZ)]
    #[assoc(id = "z")]
    #[assoc(default_name = "Z")]
    Z,
    #[assoc(key_code = KeyCode::Digit0)]
    #[assoc(id = "num_0")]
    #[assoc(default_name = "0")]
    Num0,
    #[assoc(key_code = KeyCode::Digit1)]
    #[assoc(id = "num_1")]
    #[assoc(default_name = "1")]
    Num1,
    #[assoc(key_code = KeyCode::Digit2)]
    #[assoc(id = "num_2")]
    #[assoc(default_name = "2")]
    Num2,
    #[assoc(key_code = KeyCode::Digit3)]
    #[assoc(id = "num_3")]
    #[assoc(default_name = "3")]
    Num3,
    #[assoc(key_code = KeyCode::Digit4)]
    #[assoc(id = "num_4")]
    #[assoc(default_name = "4")]
    Num4,
    #[assoc(key_code = KeyCode::Digit5)]
    #[assoc(id = "num_5")]
    #[assoc(default_name = "5")]
    Num5,
    #[assoc(key_code = KeyCode::Digit6)]
    #[assoc(id = "num_6")]
    #[assoc(default_name = "6")]
    Num6,
    #[assoc(key_code = KeyCode::Digit7)]
    #[assoc(id = "num_7")]
    #[assoc(default_name = "7")]
    Num7,
    #[assoc(key_code = KeyCode::Digit8)]
    #[assoc(id = "num_8")]
    #[assoc(default_name = "8")]
    Num8,
    #[assoc(key_code = KeyCode::Digit9)]
    #[assoc(id = "num_9")]
    #[assoc(default_name = "9")]
    Num9,
    #[assoc(key_code = KeyCode::Escape)]
    #[assoc(id = "esc")]
    #[assoc(default_name = "Esc")]
    Escape,
    #[assoc(key_code = KeyCode::F1)]
    #[assoc(id = "f1")]
    #[assoc(default_name = "F1")]
    F1,
    #[assoc(key_code = KeyCode::F2)]
    #[assoc(id = "f2")]
    #[assoc(default_name = "F2")]
    F2,
    #[assoc(key_code = KeyCode::F3)]
    #[assoc(id = "f3")]
    #[assoc(default_name = "F3")]
    F3,
    #[assoc(key_code = KeyCode::F4)]
    #[assoc(id = "f4")]
    #[assoc(default_name = "F4")]
    F4,
    #[assoc(key_code = KeyCode::F5)]
    #[assoc(id = "f5")]
    #[assoc(default_name = "F5")]
    F5,
    #[assoc(key_code = KeyCode::F6)]
    #[assoc(id = "f6")]
    #[assoc(default_name = "F6")]
    F6,
    #[assoc(key_code = KeyCode::F7)]
    #[assoc(id = "f7")]
    #[assoc(default_name = "F7")]
    F7,
    #[assoc(key_code = KeyCode::F8)]
    #[assoc(id = "f8")]
    #[assoc(default_name = "F8")]
    F8,
    #[assoc(key_code = KeyCode::F9)]
    #[assoc(id = "f9")]
    #[assoc(default_name = "F9")]
    F9,
    #[assoc(key_code = KeyCode::F10)]
    #[assoc(id = "f10")]
    #[assoc(default_name = "F10")]
    F10,
    #[assoc(key_code = KeyCode::F11)]
    #[assoc(id = "f11")]
    #[assoc(default_name = "F11")]
    F11,
    #[assoc(key_code = KeyCode::F12)]
    #[assoc(id = "f12")]
    #[assoc(default_name = "F12")]
    F12,
    #[assoc(key_code = KeyCode::F13)]
    #[assoc(id = "f13")]
    #[assoc(default_name = "F13")]
    F13,
    #[assoc(key_code = KeyCode::F14)]
    #[assoc(id = "f14")]
    #[assoc(default_name = "F14")]
    F14,
    #[assoc(key_code = KeyCode::F15)]
    #[assoc(id = "f15")]
    #[assoc(default_name = "F15")]
    F15,
    #[assoc(key_code = KeyCode::F16)]
    #[assoc(id = "f16")]
    #[assoc(default_name = "F16")]
    F16,
    #[assoc(key_code = KeyCode::F17)]
    #[assoc(id = "f17")]
    #[assoc(default_name = "F17")]
    F17,
    #[assoc(key_code = KeyCode::F18)]
    #[assoc(id = "f18")]
    #[assoc(default_name = "F18")]
    F18,
    #[assoc(key_code = KeyCode::F19)]
    #[assoc(id = "f19")]
    #[assoc(default_name = "F19")]
    F19,
    #[assoc(key_code = KeyCode::F20)]
    #[assoc(id = "f20")]
    #[assoc(default_name = "F20")]
    F20,
    #[assoc(key_code = KeyCode::F21)]
    #[assoc(id = "f21")]
    #[assoc(default_name = "F21")]
    F21,
    #[assoc(key_code = KeyCode::F22)]
    #[assoc(id = "f22")]
    #[assoc(default_name = "F22")]
    F22,
    #[assoc(key_code = KeyCode::F23)]
    #[assoc(id = "f23")]
    #[assoc(default_name = "F23")]
    F23,
    #[assoc(key_code = KeyCode::F24)]
    #[assoc(id = "f24")]
    #[assoc(default_name = "F24")]
    F24,
    #[assoc(key_code = KeyCode::Backspace)]
    #[assoc(id = "backspace")]
    #[assoc(default_name = "Backspace")]
    Backspace,
    #[assoc(key_code = KeyCode::Tab)]
    #[assoc(id = "tab")]
    #[assoc(default_name = "Tab")]
    Tab,
    #[assoc(key_code = KeyCode::Enter)]
    #[assoc(id = "enter")]
    #[assoc(default_name = "Enter")]
    Enter,
    #[assoc(key_code = KeyCode::CapsLock)]
    #[assoc(id = "caps_lock")]
    #[assoc(default_name = "Caps Lock")]
    CapsLock,
    #[assoc(key_code = KeyCode::ShiftLeft)]
    #[assoc(id = "l_shift")]
    #[assoc(default_name = "L. Shift")]
    LeftShift,
    #[assoc(key_code = KeyCode::ShiftRight)]
    #[assoc(id = "r_shift")]
    #[assoc(default_name = "R. Shift")]
    RightShift,
    #[assoc(key_code = KeyCode::ControlLeft)]
    #[assoc(id = "l_ctrl")]
    #[assoc(default_name = "L. Ctrl")]
    LeftControl,
    #[assoc(key_code = KeyCode::ControlRight)]
    #[assoc(id = "r_ctrl")]
    #[assoc(default_name = "R. Ctrl")]
    RightControl,
    #[assoc(key_code = KeyCode::AltLeft)]
    #[assoc(id = "l_alt")]
    #[assoc(default_name = "L. Alt")]
    LeftAlt,
    #[assoc(key_code = KeyCode::AltRight)]
    #[assoc(id = "r_alt")]
    #[assoc(default_name = "R. Alt")]
    RightAlt,
    #[assoc(key_code = KeyCode::SuperLeft)]
    #[assoc(id = "l_super")]
    #[assoc(default_name = "L. Super")]
    LeftSuper,
    #[assoc(key_code = KeyCode::SuperRight)]
    #[assoc(id = "r_super")]
    #[assoc(default_name = "R. Super")]
    RightSuper,
    #[assoc(key_code = KeyCode::ContextMenu)]
    #[assoc(id = "menu")]
    #[assoc(default_name = "Menu")]
    Menu,
    #[assoc(key_code = KeyCode::Space)]
    #[assoc(id = "space")]
    #[assoc(default_name = "Space")]
    Space,
    #[assoc(key_code = KeyCode::Backquote)]
    #[assoc(id = "backtick")]
    #[assoc(default_name = "`")]
    Backtick,
    #[assoc(key_code = KeyCode::Minus)]
    #[assoc(id = "minus")]
    #[assoc(default_name = "-")]
    Minus,
    #[assoc(key_code = KeyCode::Equal)]
    #[assoc(id = "equal")]
    #[assoc(default_name = "=")]
    Equal,
    #[assoc(key_code = KeyCode::BracketLeft)]
    #[assoc(id = "l_bracket")]
    #[assoc(default_name = "[")]
    LeftBracket,
    #[assoc(key_code = KeyCode::BracketRight)]
    #[assoc(id = "r_bracket")]
    #[assoc(default_name = "]")]
    RightBracket,
    #[assoc(key_code = KeyCode::Backslash)]
    #[assoc(id = "backslash")]
    #[assoc(default_name = "\\")]
    Backslash,
    #[assoc(key_code = KeyCode::Semicolon)]
    #[assoc(id = "semicolon")]
    #[assoc(default_name = ";")]
    Semicolon,
    #[assoc(key_code = KeyCode::Quote)]
    #[assoc(id = "quote")]
    #[assoc(default_name = "'")]
    Quote,
    #[assoc(key_code = KeyCode::Comma)]
    #[assoc(id = "comma")]
    #[assoc(default_name = ",")]
    Comma,
    #[assoc(key_code = KeyCode::Period)]
    #[assoc(id = "period")]
    #[assoc(default_name = ".")]
    Period,
    #[assoc(key_code = KeyCode::Slash)]
    #[assoc(id = "slash")]
    #[assoc(default_name = "/")]
    Slash,
    #[assoc(key_code = KeyCode::PrintScreen)]
    #[assoc(id = "print_screen")]
    #[assoc(default_name = "PrtSc")]
    PrintScreen,
    #[assoc(key_code = KeyCode::ScrollLock)]
    #[assoc(id = "scroll_lock")]
    #[assoc(default_name = "ScrLk")]
    ScrollLock,
    #[assoc(key_code = KeyCode::Pause)]
    #[assoc(id = "pause")]
    #[assoc(default_name = "Pause")]
    Pause,
    #[assoc(key_code = KeyCode::Insert)]
    #[assoc(id = "insert")]
    #[assoc(default_name = "Insert")]
    Insert,
    #[assoc(key_code = KeyCode::Home)]
    #[assoc(id = "home")]
    #[assoc(default_name = "Home")]
    Home,
    #[assoc(key_code = KeyCode::PageUp)]
    #[assoc(id = "page_up")]
    #[assoc(default_name = "PgUp")]
    PageUp,
    #[assoc(key_code = KeyCode::Delete)]
    #[assoc(id = "delete")]
    #[assoc(default_name = "Delete")]
    Delete,
    #[assoc(key_code = KeyCode::End)]
    #[assoc(id = "end")]
    #[assoc(default_name = "End")]
    End,
    #[assoc(key_code = KeyCode::PageDown)]
    #[assoc(id = "page_down")]
    #[assoc(default_name = "PgDn")]
    PageDown,
    #[assoc(key_code = KeyCode::ArrowRight)]
    #[assoc(id = "arrow_right")]
    #[assoc(default_name = "Right")]
    Right,
    #[assoc(key_code = KeyCode::ArrowLeft)]
    #[assoc(id = "arrow_left")]
    #[assoc(default_name = "Left")]
    Left,
    #[assoc(key_code = KeyCode::ArrowDown)]
    #[assoc(id = "arrow_down")]
    #[assoc(default_name = "Down")]
    Down,
    #[assoc(key_code = KeyCode::ArrowUp)]
    #[assoc(id = "arrow_up")]
    #[assoc(default_name = "Up")]
    Up,
    #[assoc(key_code = KeyCode::NumLock)]
    #[assoc(id = "num_lock")]
    #[assoc(default_name = "NumLk")]
    NumLock,
    #[assoc(key_code = KeyCode::NumpadDivide)]
    #[assoc(id = "numpad_divide")]
    #[assoc(default_name = "Numpad /")]
    NumpadDivide,
    #[assoc(key_code = KeyCode::NumpadMultiply)]
    #[assoc(id = "numpad_multiply")]
    #[assoc(default_name = "Numpad *")]
    NumpadMultiply,
    #[assoc(key_code = KeyCode::NumpadSubtract)]
    #[assoc(id = "numpad_subtract")]
    #[assoc(default_name = "Numpad -")]
    NumpadSubtract,
    #[assoc(key_code = KeyCode::NumpadAdd)]
    #[assoc(id = "numpad_add")]
    #[assoc(default_name = "Numpad +")]
    NumpadAdd,
    #[assoc(key_code = KeyCode::NumpadEnter)]
    #[assoc(id = "numpad_enter")]
    #[assoc(default_name = "Numpad Enter")]
    NumpadEnter,
    #[assoc(key_code = KeyCode::NumpadDecimal)]
    #[assoc(id = "numpad_decimal")]
    #[assoc(default_name = "Numpad .")]
    NumpadDecimal,
    #[assoc(key_code = KeyCode::Numpad0)]
    #[assoc(id = "numpad_0")]
    #[assoc(default_name = "Numpad 0")]
    Numpad0,
    #[assoc(key_code = KeyCode::Numpad1)]
    #[assoc(id = "numpad_1")]
    #[assoc(default_name = "Numpad 1")]
    Numpad1,
    #[assoc(key_code = KeyCode::Numpad2)]
    #[assoc(id = "numpad_2")]
    #[assoc(default_name = "Numpad 2")]
    Numpad2,
    #[assoc(key_code = KeyCode::Numpad3)]
    #[assoc(id = "numpad_3")]
    #[assoc(default_name = "Numpad 3")]
    Numpad3,
    #[assoc(key_code = KeyCode::Numpad4)]
    #[assoc(id = "numpad_4")]
    #[assoc(default_name = "Numpad 4")]
    Numpad4,
    #[assoc(key_code = KeyCode::Numpad5)]
    #[assoc(id = "numpad_5")]
    #[assoc(default_name = "Numpad 5")]
    Numpad5,
    #[assoc(key_code = KeyCode::Numpad6)]
    #[assoc(id = "numpad_6")]
    #[assoc(default_name = "Numpad 6")]
    Numpad6,
    #[assoc(key_code = KeyCode::Numpad7)]
    #[assoc(id = "numpad_7")]
    #[assoc(default_name = "Numpad 7")]
    Numpad7,
    #[assoc(key_code = KeyCode::Numpad8)]
    #[assoc(id = "numpad_8")]
    #[assoc(default_name = "Numpad 8")]
    Numpad8,
    #[assoc(key_code = KeyCode::Numpad9)]
    #[assoc(id = "numpad_9")]
    #[assoc(default_name = "Numpad 9")]
    Numpad9,
}

impl TryFrom<winit::keyboard::PhysicalKey> for KeyboardKey {
    type Error = ();

    fn try_from(key: winit::keyboard::PhysicalKey) -> Result<Self, Self::Error> {
        use winit::keyboard::PhysicalKey;
        match key {
            PhysicalKey::Code(code) => KeyboardKey::from_key_code(code).ok_or(()),
            PhysicalKey::Unidentified(_) => Err(()),
        }
    }
}

/// Represents a gamepad button.
/// Note that there might be some buttons that are not supported by the engine. In that case, they will be ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Assoc)]
#[func(pub fn as_gilrs(self) -> gilrs::Button as gilrs)]
#[func(pub fn from_gilrs(button: gilrs::Button) -> Option<Self> as gilrs)]
#[func(pub fn id(self) -> &'static str)]
#[func(pub fn from_id(id: &str) -> Option<Self> as id)]
pub enum GamepadButton {
    /// D-pad up.
    #[assoc(gilrs = gilrs::Button::DPadUp)]
    #[assoc(id = "dpad_up")]
    DPadUp,
    /// D-pad down.
    #[assoc(gilrs = gilrs::Button::DPadDown)]
    #[assoc(id = "dpad_down")]
    DPadDown,
    /// D-pad left.
    #[assoc(gilrs = gilrs::Button::DPadLeft)]
    #[assoc(id = "dpad_left")]
    DPadLeft,
    /// D-pad right.
    #[assoc(gilrs = gilrs::Button::DPadRight)]
    #[assoc(id = "dpad_right")]
    DPadRight,
    /// North face button. Corresponds to Y on Xbox and triangle on PlayStation.
    #[assoc(gilrs = gilrs::Button::North)]
    #[assoc(id = "north")]
    North,
    /// South face button. Corresponds to A on Xbox and cross on PlayStation.
    #[assoc(gilrs = gilrs::Button::South)]
    #[assoc(id = "south")]
    South,
    /// East face button. Corresponds to B on Xbox and circle on PlayStation.
    #[assoc(gilrs = gilrs::Button::East)]
    #[assoc(id = "east")]
    East,
    /// West face button. Corresponds to X on Xbox and square on PlayStation.
    #[assoc(gilrs = gilrs::Button::West)]
    #[assoc(id = "west")]
    West,
    /// Left bumper. Corresponds to LB on Xbox and L1 on PlayStation.
    #[assoc(gilrs = gilrs::Button::LeftTrigger)]
    #[assoc(id = "left_bumper")]
    LeftBumper,
    /// Right bumper. Corresponds to RB on Xbox and R1 on PlayStation.
    #[assoc(gilrs = gilrs::Button::RightTrigger)]
    #[assoc(id = "right_bumped")]
    RightBumper,
    /// Left trigger. Corresponds to LT on Xbox and L2 on PlayStation.
    #[assoc(gilrs = gilrs::Button::LeftTrigger2)]
    #[assoc(id = "left_trigger")]
    LeftTrigger,
    /// Right trigger. Corresponds to RT on Xbox and R2 on PlayStation.
    #[assoc(gilrs = gilrs::Button::RightTrigger2)]
    #[assoc(id = "right_trigger")]
    RightTrigger,
    /// Left stick button. Corresponds to LSB on Xbox and L3 on PlayStation.
    #[assoc(gilrs = gilrs::Button::LeftThumb)]
    #[assoc(id = "left_stick_click")]
    LeftStick,
    /// Right stick button. Corresponds to RSB on Xbox and R3 on PlayStation.
    #[assoc(gilrs = gilrs::Button::RightThumb)]
    #[assoc(id = "right_stick_click")]
    RightStick,
    /// Select button. Corresponds to Back/View on Xbox and Select on PlayStation.
    #[assoc(gilrs = gilrs::Button::Select)]
    #[assoc(id = "select")]
    Select,
    /// Start button. Corresponds to Start/Menu on Xbox and Start on PlayStation.
    #[assoc(gilrs = gilrs::Button::Start)]
    #[assoc(id = "start")]
    Start,
}

impl TryFrom<gilrs::Button> for GamepadButton {
    type Error = ();

    fn try_from(value: gilrs::Button) -> Result<Self, Self::Error> {
        GamepadButton::from_gilrs(value).ok_or(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Assoc)]
#[func(pub fn gilrs(self) -> gilrs::Axis)]
#[func(pub fn from_gilrs(axis: gilrs::Axis) -> Option<Self> as gilrs)]
#[func(pub fn id(self) -> &'static str)]
#[func(pub fn from_id(id: &str) -> Option<Self> as id)]
pub enum GamepadAxis {
    // Horizontal axis of the left stick. Goes from -1 (left) to 1 (right).
    #[assoc(gilrs = gilrs::Axis::LeftStickX)]
    #[assoc(id = "left_stick_x")]
    LeftStickX,
    // Vertical axis of the left stick. Goes from -1 (bottom) to 1 (top).
    #[assoc(gilrs = gilrs::Axis::LeftStickY)]
    #[assoc(id = "left_stick_y")]
    LeftStickY,
    // Horizontal axis of the right stick. Goes from -1 (left) to 1 (right).
    #[assoc(gilrs = gilrs::Axis::RightStickX)]
    #[assoc(id = "right_stick_x")]
    RightStickX,
    // Vertical axis of the right stick. Goes from -1 (bottom) to 1 (top).
    #[assoc(gilrs = gilrs::Axis::RightStickY)]
    #[assoc(id = "right_stick_y")]
    RightStickY,
    // Analog axis of the left trigger (LT/L2). Goes from 0 (no force) to 1 (full force).
    #[assoc(gilrs = gilrs::Axis::LeftZ)]
    #[assoc(id = "left_trigger")]
    LeftTrigger,
    // Analog axis of the right trigger (RT/R2). Goes from 0 (no force) to 1 (full force).
    #[assoc(gilrs = gilrs::Axis::RightZ)]
    #[assoc(id = "right_trigger")]
    RightTrigger,
}

impl TryFrom<gilrs::Axis> for GamepadAxis {
    type Error = ();

    fn try_from(value: gilrs::Axis) -> Result<Self, Self::Error> {
        GamepadAxis::from_gilrs(value).ok_or(())
    }
}

impl TryFrom<gilrs::Button> for GamepadAxis {
    type Error = ();

    fn try_from(value: gilrs::Button) -> Result<Self, Self::Error> {
        match value {
            gilrs::Button::LeftTrigger2 => Ok(Self::LeftTrigger),
            gilrs::Button::RightTrigger2 => Ok(Self::RightTrigger),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ActionMapping {
    KeyboardKey(KeyboardKey),
    GamepadButton(GamepadButton),
    GamepadAxis { axis: GamepadAxis, deadzone: f32 },
}

impl ActionMapping {
    pub fn matches_control_type(&self, control_type: ControlType) -> bool {
        match self {
            ActionMapping::KeyboardKey(_) => control_type.is_keyboard(),
            ActionMapping::GamepadButton(_) | ActionMapping::GamepadAxis { .. } => {
                control_type.is_gamepad()
            }
        }
    }
}

#[derive(Debug)]
pub enum AxisMapping {
    KeyboardButtons {
        negative: KeyboardKey,
        positive: KeyboardKey,
    },
    GamepadButtons {
        negative: GamepadButton,
        positive: GamepadButton,
    },
    GamepadAxis {
        axis: GamepadAxis,
        inverted: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlType {
    Keyboard,
    Gamepad(Option<GamepadType>),
}

impl ControlType {
    pub fn is_keyboard(self) -> bool {
        matches!(self, ControlType::Keyboard)
    }

    pub fn is_gamepad(self) -> bool {
        matches!(self, ControlType::Gamepad(_))
    }

    pub fn gamepad_type(self) -> Option<GamepadType> {
        match self {
            ControlType::Keyboard => None,
            ControlType::Gamepad(gamepad_type) => gamepad_type,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Assoc)]
#[func(pub fn id(self) -> &'static str)]
#[func(pub fn from_id(id: &str) -> Option<Self> as id)]
pub enum GamepadType {
    #[assoc(id = "xbox")]
    Xbox,
    #[assoc(id = "playstation")]
    PlayStation,
}

impl GamepadType {
    pub fn try_recognize_gamepad(gamepad: &gilrs::Gamepad) -> Option<Self> {
        if let Some(mapping_name) = gamepad.map_name() {
            if let Some(ty) = GamepadType::try_recognize_name(mapping_name) {
                return Some(ty);
            }
        }

        let os_name = gamepad.os_name();
        GamepadType::try_recognize_name(os_name)
    }

    pub fn try_recognize_name(name: &str) -> Option<Self> {
        if name.contains("PlayStation")
            || name.contains("PS")
            || name.contains("DualSense")
            || name.contains("DualShock")
        {
            Some(Self::PlayStation)
        } else if name.contains("Xbox") || name.contains("X-Box") || name.contains("XInput") {
            Some(Self::Xbox)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    #[test]
    fn from_key_code() {
        let code = KeyCode::KeyE;
        assert_eq!(KeyboardKey::from_key_code(code), Some(KeyboardKey::E));
    }
}
