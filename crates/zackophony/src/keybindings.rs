use std::str::FromStr;

use global_hotkey::hotkey::HotKey;
use serde_json::{Map, Value};
use strum_macros::EnumIter;

use crate::ui::UiAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter)]
pub enum HotkeyBehavior {
	Press,
	Release,
}

impl HotkeyBehavior {
	pub fn name(&self) -> &'static str {
		match self {
			HotkeyBehavior::Press => "Press",
			HotkeyBehavior::Release => "Release",
		}
	}

	pub fn from_name(name: &str) -> Option<HotkeyBehavior> {
		match name {
			"Press" => Some(HotkeyBehavior::Press),
			"Release" => Some(HotkeyBehavior::Release),
			_ => None,
		}
	}
}

pub struct Keybinding {
	pub key: HotKey,
	pub behavior: HotkeyBehavior,
	pub action: UiAction,
	pub global: bool,
}

impl Keybinding {
	pub fn serialize(&self) -> String {
		format!(
			r#"{{
	"key": "{key}",
	"global": {global},
	"behavior": "{behavior}",
	"action": "{action}"
}}"#,
			key = self.key,
			behavior = self.behavior.name(),
			action = self.action.name(),
			global = self.global,
		)
	}

	pub fn deserialize(text: &str) -> anyhow::Result<Keybinding> {
		let json: Map<String, Value> = serde_json::from_str(text)?;
		Ok(Keybinding {
			key: HotKey::from_str(
				json.get("key")
					.ok_or_else(|| anyhow::anyhow!("missing key"))?
					.as_str()
					.ok_or_else(|| anyhow::anyhow!("key is not a string"))?,
			)?,
			global: json
				.get("global")
				.ok_or_else(|| anyhow::anyhow!("missing global"))?
				.as_bool()
				.ok_or_else(|| anyhow::anyhow!("global is not a boolean"))?,
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
