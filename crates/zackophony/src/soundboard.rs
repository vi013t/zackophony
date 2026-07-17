use std::{str::FromStr, sync::LazyLock};

use eframe::egui::Color32;
use rodio::{Device, DeviceTrait};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
	audio::{AudioPlayer, SoundData},
	theme::{CATPPUCCIN_MOCHA, THEMES, Theme},
};

#[derive(Clone)]
pub struct Soundboard {
	pub pages: Vec<SoundPage>,
}

impl Soundboard {
	pub fn serialize(&self) -> String {
		format!(
			r#"{{
	"pages": [{pages}]
}}"#,
			pages = self
				.pages
				.iter()
				.map(|page| page.serialize().lines().map(|line| format!("\t{line}")).collect::<Vec<_>>().join("\n"))
				.collect::<Vec<_>>()
				.join(",\n")
		)
	}

	pub fn deserialize(text: &str) -> anyhow::Result<Soundboard> {
		let json: Map<String, Value> = serde_json::from_str(text)?;
		Ok(Soundboard {
			pages: json
				.get("pages")
				.ok_or_else(|| anyhow::anyhow!("missing pages"))?
				.as_array()
				.ok_or_else(|| anyhow::anyhow!("pages is not an array"))?
				.iter()
				.map(|value| SoundPage::deserialize(&value.to_string()))
				.collect::<Result<Vec<_>, _>>()?,
		})
	}
}

#[derive(Clone)]
pub struct SoundPage {
	pub name: String,
	pub sounds: Vec<SoundTreeNode>,
}

impl SoundPage {
	pub fn sounds(&self) -> Vec<&Sound> {
		self.sounds
			.iter()
			.flat_map(|s| match s {
				SoundTreeNode::Sound(sound) => vec![sound],
				SoundTreeNode::Group(sound_folder) => sound_folder.sounds(),
			})
			.collect()
	}

	pub fn sound_nodes(&self) -> Vec<&SoundTreeNode> {
		self.sounds
			.iter()
			.flat_map(|s| match s {
				SoundTreeNode::Sound(_) => vec![s],
				SoundTreeNode::Group(sound_folder) => {
					let mut rest = sound_folder.sound_nodes();
					rest.push(s);
					rest
				},
			})
			.collect()
	}

	pub fn serialize(&self) -> String {
		format!(
			r#"{{
	"name": "{name}",
	"sounds": [
	{sounds}
	]
}}"#,
			name = self.name,
			sounds = self
				.sounds
				.iter()
				.map(|sound| sound.serialize().lines().map(|line| format!("\t{line}")).collect::<Vec<_>>().join("\n"))
				.collect::<Vec<_>>()
				.join(",\n"),
		)
	}

	pub fn deserialize(text: &str) -> anyhow::Result<SoundPage> {
		let json: Map<String, Value> = serde_json::from_str(text)?;
		Ok(SoundPage {
			name: json
				.get("name")
				.ok_or_else(|| anyhow::anyhow!("missing name"))?
				.as_str()
				.ok_or_else(|| anyhow::anyhow!("name is not a string"))?
				.to_owned(),
			sounds: json
				.get("sounds")
				.ok_or_else(|| anyhow::anyhow!("missing sounds"))?
				.as_array()
				.ok_or_else(|| anyhow::anyhow!("sounds is not an array"))?
				.iter()
				.map(|value| SoundTreeNode::deserialize(&value.to_string()))
				.collect::<Result<Vec<_>, _>>()?,
		})
	}
}

impl Soundboard {
	pub fn new() -> anyhow::Result<Soundboard> {
		Ok(Soundboard {
			pages: vec![SoundPage {
				name: String::new(),
				sounds: vec![
					SoundTreeNode::Sound(Sound {
						name: "grooming".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/youre_grooming_me.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::BABY.to_owned()),
						id: Uuid::new_v4(),
					}),
					SoundTreeNode::Sound(Sound {
						name: "ZERO".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/zero.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::CROSSHAIR.to_owned()),
						id: Uuid::new_v4(),
					}),
					SoundTreeNode::Group(SoundGroup {
						name: "schizophrenia".to_owned(),
						icon: Some(egui_phosphor::regular::GHOST.to_owned()),
						id: Uuid::new_v4(),
						sounds: vec![
							SoundTreeNode::Sound(Sound {
								name: "skill check".to_owned(),
								data: SoundData::new(include_bytes!("../assets/sounds/default/skill_check.mp3").to_vec())?,
								icon: Some(egui_phosphor::regular::SKULL.to_owned()),
								id: Uuid::new_v4(),
							}),
							SoundTreeNode::Sound(Sound {
								name: "steps".to_owned(),
								data: SoundData::new(include_bytes!("../assets/sounds/default/steps.mp3").to_vec())?,
								icon: Some(egui_phosphor::regular::FOOTPRINTS.to_owned()),
								id: Uuid::new_v4(),
							}),
						],
					}),
					SoundTreeNode::Sound(Sound {
						name: "not real".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/not_real.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::GAME_CONTROLLER.to_owned()),
						id: Uuid::new_v4(),
					}),
					SoundTreeNode::Sound(Sound {
						name: "never again".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/never_again.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::FLASHLIGHT.to_owned()),
						id: Uuid::new_v4(),
					}),
					SoundTreeNode::Sound(Sound {
						name: "this button?".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/this_button.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::KEYBOARD.to_owned()),
						id: Uuid::new_v4(),
					}),
				],
			}],
		})
	}

	pub fn sounds(&self) -> Vec<&Sound> {
		self.pages.iter().flat_map(|p| p.sounds()).collect()
	}

	pub fn sound_nodes(&self) -> Vec<&SoundTreeNode> {
		self.pages.iter().flat_map(|p| p.sound_nodes()).collect()
	}

	pub fn get_sound_node(&self, id: Uuid) -> Option<&SoundTreeNode> {
		self.sound_nodes().iter().find(|sound| sound.id() == id).copied()
	}
}

#[derive(Debug, Clone)]
pub struct SoundGroup {
	pub icon: Option<String>,
	pub sounds: Vec<SoundTreeNode>,
	pub name: String,
	pub id: Uuid,
}

impl SoundGroup {
	pub fn sounds(&self) -> Vec<&Sound> {
		self.sounds
			.iter()
			.flat_map(|s| match s {
				SoundTreeNode::Sound(sound) => vec![sound],
				SoundTreeNode::Group(sound_folder) => sound_folder.sounds(),
			})
			.collect()
	}

	pub fn sound_nodes(&self) -> Vec<&SoundTreeNode> {
		self.sounds
			.iter()
			.flat_map(|s| match s {
				SoundTreeNode::Sound(_) => vec![s],
				SoundTreeNode::Group(sound_folder) => {
					let mut rest = sound_folder.sound_nodes();
					rest.push(s);
					rest
				},
			})
			.collect()
	}

	pub fn serialize(&self) -> String {
		format!(
			r#"{{
	"name": "{name}",
	"icon": {icon},
	"id": "{id}",
	"sounds": [
	{sounds}
	]
}}"#,
			name = self.name,
			icon = self.icon.as_ref().map(|icon| format!(r#""{icon}""#)).unwrap_or_else(|| "null".to_owned()),
			sounds = self
				.sounds
				.iter()
				.map(|sound| sound.serialize().lines().map(|line| format!("\t{line}")).collect::<Vec<_>>().join("\n"))
				.collect::<Vec<_>>()
				.join(",\n"),
			id = self.id,
		)
	}

	pub fn deserialize(text: &str) -> anyhow::Result<SoundGroup> {
		let json: Map<String, Value> = serde_json::from_str(text)?;
		Ok(SoundGroup {
			name: json
				.get("name")
				.ok_or_else(|| anyhow::anyhow!("missing name"))?
				.as_str()
				.ok_or_else(|| anyhow::anyhow!("name is not a string"))?
				.to_owned(),
			icon: json.get("icon").ok_or_else(|| anyhow::anyhow!("missing icon"))?.as_str().map(|s| s.to_owned()),
			id: Uuid::from_str(
				json.get("id")
					.ok_or_else(|| anyhow::anyhow!("missing id"))?
					.as_str()
					.ok_or_else(|| anyhow::anyhow!("id is not a string"))?,
			)?,
			sounds: json
				.get("sounds")
				.ok_or_else(|| anyhow::anyhow!("missing sounds"))?
				.as_array()
				.ok_or_else(|| anyhow::anyhow!("sounds is not an array"))?
				.iter()
				.map(|value| SoundTreeNode::deserialize(&value.to_string()))
				.collect::<Result<Vec<_>, _>>()?,
		})
	}
}

#[derive(Debug, Clone)]
pub enum SoundTreeNode {
	Sound(Sound),
	Group(SoundGroup),
}

impl SoundTreeNode {
	pub fn name(&self) -> &String {
		match self {
			SoundTreeNode::Sound(sound) => &sound.name,
			SoundTreeNode::Group(sound_folder) => &sound_folder.name,
		}
	}

	pub fn serialize(&self) -> String {
		match self {
			SoundTreeNode::Group(group) => group.serialize(),
			SoundTreeNode::Sound(sound) => sound.serialize(),
		}
	}

	pub fn deserialize(text: &str) -> anyhow::Result<SoundTreeNode> {
		let json: Map<String, Value> = serde_json::from_str(text)?;
		Ok(if json.contains_key("sounds") {
			SoundTreeNode::Group(SoundGroup::deserialize(text)?)
		} else {
			SoundTreeNode::Sound(Sound::deserialize(text)?)
		})
	}

	pub fn id(&self) -> Uuid {
		match self {
			SoundTreeNode::Sound(sound) => sound.id,
			SoundTreeNode::Group(group) => group.id,
		}
	}

	pub fn icon(&self) -> Option<&String> {
		match self {
			SoundTreeNode::Sound(sound) => sound.icon.as_ref(),
			SoundTreeNode::Group(sound_folder) => sound_folder.icon.as_ref(),
		}
	}

	pub fn border_color(&self, theme: &LazyLock<Theme>, hovered: bool) -> Color32 {
		if hovered { self.border_hover_color(theme) } else { self.border_regular_color(theme) }
	}

	pub fn background_color(&self, theme: &LazyLock<Theme>, hovered: bool) -> Color32 {
		if hovered {
			self.background_color_hover(theme)
		} else {
			self.background_regular_color(theme)
		}
	}

	pub fn border_regular_color(&self, theme: &LazyLock<Theme>) -> Color32 {
		match self {
			SoundTreeNode::Sound(_) => theme.sound_border(),
			SoundTreeNode::Group(_) => theme.sound_group_border(),
		}
	}

	pub fn border_hover_color(&self, theme: &LazyLock<Theme>) -> Color32 {
		match self {
			SoundTreeNode::Sound(_) => theme.sound_border_hover(),
			SoundTreeNode::Group(_) => theme.sound_group_border_hover(),
		}
	}

	pub fn background_regular_color(&self, theme: &LazyLock<Theme>) -> Color32 {
		match self {
			SoundTreeNode::Sound(_) => theme.sound_background(),
			SoundTreeNode::Group(_) => theme.sound_group_background(),
		}
	}

	pub fn background_color_hover(&self, theme: &LazyLock<Theme>) -> Color32 {
		match self {
			SoundTreeNode::Sound(_) => theme.sound_background_hover(),
			SoundTreeNode::Group(_) => theme.sound_group_background_hover(),
		}
	}
}

#[derive(Clone)]
pub struct Sound {
	pub name: String,
	pub icon: Option<String>,
	pub data: SoundData,
	pub id: Uuid,
}

impl Sound {
	pub fn serialize(&self) -> String {
		format!(
			r#"{{
	"name": "{name}",
	"icon": {icon},
	"data": [{data}],
	"id": "{id}"
}}"#,
			name = self.name,
			icon = self.icon.as_ref().map(|icon| format!(r#""{icon}""#)).unwrap_or_else(|| "null".to_owned()),
			data = self.data.bytes().iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", "),
			id = self.id,
		)
	}

	pub fn deserialize(text: &str) -> anyhow::Result<Sound> {
		let json: Map<String, Value> = serde_json::from_str(text)?;
		Ok(Sound {
			name: json
				.get("name")
				.ok_or_else(|| anyhow::anyhow!("missing name"))?
				.as_str()
				.ok_or_else(|| anyhow::anyhow!("name is not a string"))?
				.to_owned(),
			icon: json.get("icon").ok_or_else(|| anyhow::anyhow!("missing icon"))?.as_str().map(|s| s.to_owned()),
			id: Uuid::from_str(
				json.get("id")
					.ok_or_else(|| anyhow::anyhow!("missing id"))?
					.as_str()
					.ok_or_else(|| anyhow::anyhow!("id is not a string"))?,
			)?,
			data: SoundData::new(
				json.get("data")
					.ok_or_else(|| anyhow::anyhow!("missing data"))?
					.as_array()
					.ok_or_else(|| anyhow::anyhow!("name is not a string"))?
					.iter()
					.map(|value| value.as_u64().map(|value| value as u8).ok_or_else(|| anyhow::anyhow!("byte is not a number")))
					.collect::<Result<Vec<_>, _>>()?,
			)?,
		})
	}
}

impl std::fmt::Debug for Sound {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "Sound {{ name: {:?}, icon: {:?}, id: {:?} }}", self.name, self.icon, self.id)
	}
}

pub struct SoundboardConfig {
	pub(crate) theme: &'static LazyLock<Theme>,
	pub(crate) virtual_cable_device: Option<Device>,
	pub(crate) input_device: Option<Device>,
	pub(crate) output_device: Option<Device>,
	pub(crate) background_opacity: f32,
	pub(crate) soundboard: Soundboard,
}

impl Default for SoundboardConfig {
	fn default() -> Self {
		let app_dir = directories::ProjectDirs::from("", "", "Zackophony").unwrap();
		let dir = app_dir.config_dir();
		let config_file = dir.join("config.json");
		if config_file.is_file()
			&& let Ok(contents) = std::fs::read_to_string(config_file)
			&& let Ok(json) = serde_json::from_str::<serde_json::Map<String, Value>>(&contents)
		{
			let theme = THEMES
				.iter()
				.find(|theme| json.get("theme").and_then(|theme| theme.as_str()).is_some_and(|name| name == theme.name()));
			let virtual_cable_device = AudioPlayer::output_devices().into_iter().find(|device| {
				device.id().is_ok_and(|id| {
					json.get("virtual_cable_device")
						.and_then(|device| device.as_str())
						.is_some_and(|device| device == id.to_string())
				})
			});
			let output_device = AudioPlayer::output_devices().into_iter().find(|device| {
				device
					.id()
					.is_ok_and(|id| json.get("output_device").and_then(|device| device.as_str()).is_some_and(|device| device == id.to_string()))
			});
			let input_device = AudioPlayer::input_devices().into_iter().find(|device| {
				device
					.id()
					.is_ok_and(|id| json.get("input_device").and_then(|device| device.as_str()).is_some_and(|device| device == id.to_string()))
			});
			let background_opacity = json.get("background_opacity").and_then(|opacity| opacity.as_u64()).map(|i| i as f32);
			let soundboard = json.get("soundboard").and_then(|sb| {
				let soundboard = Soundboard::deserialize(&sb.to_string());
				if let Err(e) = &soundboard {
					dbg!(e);
				}
				soundboard.ok()
			});

			if let Some(theme) = theme
				&& let Some(background_opacity) = background_opacity
				&& let Some(soundboard) = soundboard
			{
				return SoundboardConfig {
					theme,
					virtual_cable_device,
					input_device,
					output_device,
					background_opacity,
					soundboard,
				};
			};
		}

		let preferences = SoundboardConfig {
			theme: &CATPPUCCIN_MOCHA,
			virtual_cable_device: AudioPlayer::default_virtual_device(),
			input_device: AudioPlayer::default_input_device(),
			output_device: AudioPlayer::default_output_device(),
			background_opacity: 100.,
			soundboard: Soundboard::new().unwrap(),
		};
		let _ = preferences.save_to_disk();
		preferences
	}
}

impl SoundboardConfig {
	pub fn save_to_disk(&self) -> std::io::Result<()> {
		let app_dir = directories::ProjectDirs::from("", "", "Zackophony").unwrap();
		let dir = app_dir.config_dir();
		std::fs::create_dir_all(dir)?;
		std::fs::write(dir.join("config.json"), self.serialize())
	}

	pub fn serialize(&self) -> String {
		format!(
			r#"{{
	"theme": "{theme}",
	"output_device": {output_device},
	"input_device": {input_device},
	"virtual_cable_device": {virtual_cable_device},
	"background_opacity": {background_opacity},
	"soundboard": {soundboard}
}}"#,
			theme = self.theme.name(),
			virtual_cable_device = self
				.virtual_cable_device
				.as_ref()
				.and_then(|device| device.id().ok().map(|id| format!(r#""{id}""#)))
				.unwrap_or_else(|| "null".to_owned()),
			input_device = self
				.input_device
				.as_ref()
				.and_then(|device| device.id().ok().map(|id| format!(r#""{id}""#)))
				.unwrap_or_else(|| "null".to_owned()),
			output_device = self
				.output_device
				.as_ref()
				.and_then(|device| device.id().ok().map(|id| format!(r#""{id}""#)))
				.unwrap_or_else(|| "null".to_owned()),
			background_opacity = self.background_opacity.floor() as u8,
			soundboard = self
				.soundboard
				.serialize()
				.lines()
				.map(|line| format!("\t{line}"))
				.collect::<Vec<_>>()
				.join("\n")
				.trim_start()
		)
	}
}
