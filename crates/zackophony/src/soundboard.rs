use std::sync::LazyLock;

use eframe::egui::Color32;

use crate::{
	audio::SoundData,
	theme::{CATPPUCCIN_MOCHA, Theme},
};

#[derive(Clone)]
pub struct Soundboard {
	pub pages: Vec<SoundPage>,
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
					}),
					SoundTreeNode::Sound(Sound {
						name: "ZERO".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/zero.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::CROSSHAIR.to_owned()),
					}),
					SoundTreeNode::Group(SoundGroup {
						name: "schizophrenia".to_owned(),
						icon: Some(egui_phosphor::regular::SMILEY_ANGRY.to_owned()),
						sounds: vec![
							SoundTreeNode::Sound(Sound {
								name: "skill check".to_owned(),
								data: SoundData::new(include_bytes!("../assets/sounds/default/skill_check.mp3").to_vec())?,
								icon: Some(egui_phosphor::regular::SKULL.to_owned()),
							}),
							SoundTreeNode::Sound(Sound {
								name: "steps".to_owned(),
								data: SoundData::new(include_bytes!("../assets/sounds/default/steps.mp3").to_vec())?,
								icon: Some(egui_phosphor::regular::FOOTPRINTS.to_owned()),
							}),
						],
					}),
					SoundTreeNode::Sound(Sound {
						name: "not real".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/not_real.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::GAME_CONTROLLER.to_owned()),
					}),
					SoundTreeNode::Sound(Sound {
						name: "never again".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/never_again.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::FLASHLIGHT.to_owned()),
					}),
					SoundTreeNode::Sound(Sound {
						name: "this button?".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/this_button.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::KEYBOARD.to_owned()),
					}),
				],
			}],
		})
	}

	pub fn sounds(&self) -> Vec<&Sound> {
		self.pages.iter().flat_map(|p| p.sounds()).collect()
	}
}

#[derive(Clone)]
pub struct SoundGroup {
	pub icon: Option<String>,
	pub sounds: Vec<SoundTreeNode>,
	pub name: String,
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
}

#[derive(Clone)]
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
}

pub struct Preferences {
	pub(crate) theme: &'static LazyLock<Theme>,
}

impl Default for Preferences {
	fn default() -> Self {
		Preferences { theme: &CATPPUCCIN_MOCHA }
	}
}
