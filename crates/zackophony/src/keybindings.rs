use std::{collections::HashMap, str::FromStr};

use eframe::egui::{Key, Modifiers};
use global_hotkey::hotkey::{Code, HotKey};
use serde_json::{Map, Value};
use strum_macros::EnumIter;
use uuid::Uuid;

use crate::ui::UiAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter)]
pub enum HotkeyBehavior {
	Press,
	Release,
}

impl HotkeyBehavior {
	pub fn name(&self) -> &'static str {
		match self {
			HotkeyBehavior::Press => "Pressed",
			HotkeyBehavior::Release => "Released",
		}
	}

	pub fn from_name(name: &str) -> Option<HotkeyBehavior> {
		match name {
			"Pressed" => Some(HotkeyBehavior::Press),
			"Released" => Some(HotkeyBehavior::Release),
			_ => None,
		}
	}
}

#[derive(Clone, PartialEq, Eq)]
pub struct Keybinding {
	pub key: LocalKeybinding,
	pub behavior: HotkeyBehavior,
	pub action: UiAction,
	pub global: bool,
	pub id: Uuid,
	pub arguments: HashMap<String, KeybindingArgumentValue>,
}

impl Keybinding {
	pub fn serialize(&self) -> String {
		format!(
			r#"{{
	"key": "{key}",
	"global": {global},
	"behavior": "{behavior}",
	"action": "{action}",
	"id": "{id}",
	"arguments": {{{arguments}}}
}}"#,
			key = self.key,
			behavior = self.behavior.name(),
			action = self.action.name(),
			global = self.global,
			id = self.id,
			arguments = self
				.arguments
				.iter()
				.map(|(name, value)| format!(r#""{name}": {}"#, value.serialize()))
				.collect::<Vec<_>>()
				.join(", ")
		)
	}

	pub fn deserialize(text: &str) -> anyhow::Result<Keybinding> {
		let json: Map<String, Value> = serde_json::from_str(text)?;
		Ok(Keybinding {
			key: LocalKeybinding::from_str(
				json.get("key")
					.ok_or_else(|| anyhow::anyhow!("missing key"))?
					.as_str()
					.ok_or_else(|| anyhow::anyhow!("key is not a string"))?,
			)?,
			id: Uuid::from_str(
				json.get("id")
					.ok_or_else(|| anyhow::anyhow!("missing id"))?
					.as_str()
					.ok_or_else(|| anyhow::anyhow!("id is not a string"))?,
			)?,
			global: json
				.get("global")
				.ok_or_else(|| anyhow::anyhow!("missing global"))?
				.as_bool()
				.ok_or_else(|| anyhow::anyhow!("global is not a boolean"))?,
			arguments: json
				.get("arguments")
				.ok_or_else(|| anyhow::anyhow!("missing arguments"))?
				.as_object()
				.ok_or_else(|| anyhow::anyhow!("arguments is not an object"))?
				.iter()
				.map(|(name, value)| Some((name.clone(), KeybindingArgumentValue::deserialize(&value.to_string())?)))
				.collect::<Option<HashMap<String, KeybindingArgumentValue>>>()
				.ok_or_else(|| anyhow::anyhow!("failed to deserialize arguments"))?,
			behavior: HotkeyBehavior::from_name(
				json.get("behavior")
					.ok_or_else(|| anyhow::anyhow!("missing behavior"))?
					.as_str()
					.ok_or_else(|| anyhow::anyhow!("key is not a string"))?,
			)
			.ok_or_else(|| anyhow::anyhow!("invalid hotkey behavior"))?,
			action: UiAction::from_name(
				json.get("action")
					.ok_or_else(|| anyhow::anyhow!("missing action"))?
					.as_str()
					.ok_or_else(|| anyhow::anyhow!("action is not a string"))?,
			)
			.ok_or_else(|| anyhow::anyhow!("invalid hotkey action"))?,
		})
	}
}

#[derive(Debug, Clone)]
pub struct LocalKeybinding {
	pub key: Key,
	pub modifiers: Modifiers,
}

impl PartialEq for LocalKeybinding {
	fn eq(&self, other: &Self) -> bool {
		self.key == other.key && self.modifiers.ctrl == other.modifiers.ctrl && self.modifiers.alt == other.modifiers.alt && self.modifiers.shift == other.modifiers.shift
	}
}

impl Eq for LocalKeybinding {}

impl LocalKeybinding {
	pub fn to_global(&self) -> Option<HotKey> {
		let code = egui_key_to_code(self.key)?;

		let mut modifiers = global_hotkey::hotkey::Modifiers::empty();

		if self.modifiers.ctrl {
			modifiers |= global_hotkey::hotkey::Modifiers::CONTROL;
		}

		if self.modifiers.shift {
			modifiers |= global_hotkey::hotkey::Modifiers::SHIFT;
		}

		if self.modifiers.alt {
			modifiers |= global_hotkey::hotkey::Modifiers::ALT;
		}

		Some(HotKey::new(Some(modifiers), code))
	}
}

impl FromStr for LocalKeybinding {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let parts = s.replace(" ", "").split('+').map(|p| p.trim().to_uppercase()).collect::<Vec<_>>();

		if parts.is_empty() {
			anyhow::bail!("Empty keybinding");
		}

		let key_str = parts.last().ok_or_else(|| anyhow::anyhow!("Missing key"))?;

		let key = string_to_key(key_str).ok_or_else(|| anyhow::anyhow!("Unknown key: {}", key_str))?;

		let mut modifiers = Modifiers::default();

		for modifier in &parts[..parts.len() - 1] {
			match modifier.as_str() {
				"CTRL" | "CONTROL" => modifiers.ctrl = true,
				"SHIFT" => modifiers.shift = true,
				"ALT" | "OPTION" => modifiers.alt = true,
				"META" | "CMD" | "COMMAND" | "WIN" | "WINDOWS" => {
					modifiers.command = true;
					modifiers.mac_cmd = true;
				},
				other => {
					anyhow::bail!("Unknown modifier: {}", other);
				},
			}
		}

		Ok(Self { key, modifiers })
	}
}

impl std::fmt::Display for LocalKeybinding {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let mut parts = Vec::new();

		if self.modifiers.ctrl {
			parts.push("Ctrl");
		}
		if self.modifiers.shift {
			parts.push("Shift");
		}
		if self.modifiers.alt {
			parts.push("Alt");
		}

		parts.push(key_to_string(self.key));

		write!(f, "{}", parts.join(" + "))
	}
}

fn egui_key_to_code(key: Key) -> Option<Code> {
	Some(match key {
		Key::A => Code::KeyA,
		Key::B => Code::KeyB,
		Key::C => Code::KeyC,
		Key::D => Code::KeyD,
		Key::E => Code::KeyE,
		Key::F => Code::KeyF,
		Key::G => Code::KeyG,
		Key::H => Code::KeyH,
		Key::I => Code::KeyI,
		Key::J => Code::KeyJ,
		Key::K => Code::KeyK,
		Key::L => Code::KeyL,
		Key::M => Code::KeyM,
		Key::N => Code::KeyN,
		Key::O => Code::KeyO,
		Key::P => Code::KeyP,
		Key::Q => Code::KeyQ,
		Key::R => Code::KeyR,
		Key::S => Code::KeyS,
		Key::T => Code::KeyT,
		Key::U => Code::KeyU,
		Key::V => Code::KeyV,
		Key::W => Code::KeyW,
		Key::X => Code::KeyX,
		Key::Y => Code::KeyY,
		Key::Z => Code::KeyZ,

		Key::Backtick => Code::Backquote,
		Key::Num0 => Code::Digit0,
		Key::Num1 => Code::Digit1,
		Key::Num2 => Code::Digit2,
		Key::Num3 => Code::Digit3,
		Key::Num4 => Code::Digit4,
		Key::Num5 => Code::Digit5,
		Key::Num6 => Code::Digit6,
		Key::Num7 => Code::Digit7,
		Key::Num8 => Code::Digit8,
		Key::Num9 => Code::Digit9,

		Key::Space => Code::Space,
		Key::Enter => Code::Enter,
		Key::Escape => Code::Escape,
		Key::Tab => Code::Tab,
		Key::Backspace => Code::Backspace,

		Key::ArrowUp => Code::ArrowUp,
		Key::ArrowDown => Code::ArrowDown,
		Key::ArrowLeft => Code::ArrowLeft,
		Key::ArrowRight => Code::ArrowRight,

		Key::F1 => Code::F1,
		Key::F2 => Code::F2,
		Key::F3 => Code::F3,
		Key::F4 => Code::F4,
		Key::F5 => Code::F5,
		Key::F6 => Code::F6,
		Key::F7 => Code::F7,
		Key::F8 => Code::F8,
		Key::F9 => Code::F9,
		Key::F10 => Code::F10,
		Key::F11 => Code::F11,
		Key::F12 => Code::F12,

		_ => return None,
	})
}

fn key_to_string(key: Key) -> &'static str {
	match key {
		Key::A => "A",
		Key::B => "B",
		Key::C => "C",
		Key::D => "D",
		Key::E => "E",
		Key::F => "F",
		Key::G => "G",
		Key::H => "H",
		Key::I => "I",
		Key::J => "J",
		Key::K => "K",
		Key::L => "L",
		Key::M => "M",
		Key::N => "N",
		Key::O => "O",
		Key::P => "P",
		Key::Q => "Q",
		Key::R => "R",
		Key::S => "S",
		Key::T => "T",
		Key::U => "U",
		Key::V => "V",
		Key::W => "W",
		Key::X => "X",
		Key::Y => "Y",
		Key::Z => "Z",

		Key::Backtick => "`",
		Key::Num0 => "0",
		Key::Num1 => "1",
		Key::Num2 => "2",
		Key::Num3 => "3",
		Key::Num4 => "4",
		Key::Num5 => "5",
		Key::Num6 => "6",
		Key::Num7 => "7",
		Key::Num8 => "8",
		Key::Num9 => "9",

		Key::Space => "Space",
		Key::Enter => "Enter",
		Key::Escape => "Esc",
		Key::Tab => "Tab",
		Key::Backspace => "Backspace",

		Key::ArrowUp => "Up",
		Key::ArrowDown => "Down",
		Key::ArrowLeft => "Left",
		Key::ArrowRight => "Right",

		Key::F1 => "F1",
		Key::F2 => "F2",
		Key::F3 => "F3",
		Key::F4 => "F4",
		Key::F5 => "F5",
		Key::F6 => "F6",
		Key::F7 => "F7",
		Key::F8 => "F8",
		Key::F9 => "F9",
		Key::F10 => "F10",
		Key::F11 => "F11",
		Key::F12 => "F12",

		_ => "UNKNOWN",
	}
}

fn string_to_key(s: &str) -> Option<Key> {
	Some(match s.to_lowercase().as_str() {
		"a" => Key::A,
		"b" => Key::B,
		"c" => Key::C,
		"d" => Key::D,
		"e" => Key::E,
		"f" => Key::F,
		"g" => Key::G,
		"h" => Key::H,
		"i" => Key::I,
		"j" => Key::J,
		"k" => Key::K,
		"l" => Key::L,
		"m" => Key::M,
		"n" => Key::N,
		"o" => Key::O,
		"p" => Key::P,
		"q" => Key::Q,
		"r" => Key::R,
		"s" => Key::S,
		"t" => Key::T,
		"u" => Key::U,
		"v" => Key::V,
		"w" => Key::W,
		"x" => Key::X,
		"y" => Key::Y,
		"z" => Key::Z,

		"`" => Key::Backtick,
		"0" => Key::Num0,
		"1" => Key::Num1,
		"2" => Key::Num2,
		"3" => Key::Num3,
		"4" => Key::Num4,
		"5" => Key::Num5,
		"6" => Key::Num6,
		"7" => Key::Num7,
		"8" => Key::Num8,
		"9" => Key::Num9,

		"space" => Key::Space,
		"enter" => Key::Enter,
		"esc" | "escape" => Key::Escape,
		"tab" => Key::Tab,
		"backspace" => Key::Backspace,

		"up" => Key::ArrowUp,
		"down" => Key::ArrowDown,
		"left" => Key::ArrowLeft,
		"right" => Key::ArrowRight,

		"f1" => Key::F1,
		"f2" => Key::F2,
		"f3" => Key::F3,
		"f4" => Key::F4,
		"f5" => Key::F5,
		"f6" => Key::F6,
		"f7" => Key::F7,
		"f8" => Key::F8,
		"f9" => Key::F9,
		"f10" => Key::F10,
		"f11" => Key::F11,
		"f12" => Key::F12,

		_ => return None,
	})
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, EnumIter)]
pub enum ArgumentType {
	Sound,
}

pub struct KeybindingArgument {
	pub argument_type: ArgumentType,
	pub name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeybindingArgumentValue {
	Uuid(Uuid),
}

impl KeybindingArgumentValue {
	pub fn serialize(&self) -> String {
		match self {
			Self::Uuid(uuid) => format!(r#""{uuid}""#),
		}
	}

	pub fn deserialize(text: &str) -> Option<KeybindingArgumentValue> {
		Some(KeybindingArgumentValue::Uuid(Uuid::from_str(text.trim_matches('"')).ok()?))
	}
}
