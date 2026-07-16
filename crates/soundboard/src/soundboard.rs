use crate::audio::SoundData;

#[derive(Clone)]
pub struct Soundboard {
	pub pages: Vec<SoundPage>,
}

#[derive(Clone)]
pub struct SoundPage {
	pub name: String,
	pub sounds: Vec<RadialButton>,
}

impl SoundPage {
	pub fn sounds(&self) -> Vec<&Sound> {
		self.sounds
			.iter()
			.flat_map(|s| match s {
				RadialButton::Sound(sound) => vec![sound],
				RadialButton::Folder(sound_folder) => sound_folder.sounds(),
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
					RadialButton::Sound(Sound {
						name: "grooming".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/youre_grooming_me.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::BABY.to_owned()),
					}),
					RadialButton::Sound(Sound {
						name: "ZERO".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/zero.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::CROSSHAIR.to_owned()),
					}),
					RadialButton::Folder(SoundFolder {
						name: "schizophrenia".to_owned(),
						icon: Some(egui_phosphor::regular::SMILEY_ANGRY.to_owned()),
						sounds: vec![
							RadialButton::Sound(Sound {
								name: "skill check".to_owned(),
								data: SoundData::new(include_bytes!("../assets/sounds/default/skill_check.mp3").to_vec())?,
								icon: Some(egui_phosphor::regular::SKULL.to_owned()),
							}),
							RadialButton::Sound(Sound {
								name: "steps".to_owned(),
								data: SoundData::new(include_bytes!("../assets/sounds/default/steps.mp3").to_vec())?,
								icon: Some(egui_phosphor::regular::FOOTPRINTS.to_owned()),
							}),
						],
					}),
					RadialButton::Sound(Sound {
						name: "not real".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/not_real.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::GAME_CONTROLLER.to_owned()),
					}),
					RadialButton::Sound(Sound {
						name: "never again".to_owned(),
						data: SoundData::new(include_bytes!("../assets/sounds/default/never_again.mp3").to_vec())?,
						icon: Some(egui_phosphor::regular::FLASHLIGHT.to_owned()),
					}),
					RadialButton::Sound(Sound {
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
pub struct SoundFolder {
	pub icon: Option<String>,
	pub sounds: Vec<RadialButton>,
	pub name: String,
}

impl SoundFolder {
	pub fn sounds(&self) -> Vec<&Sound> {
		self.sounds
			.iter()
			.flat_map(|s| match s {
				RadialButton::Sound(sound) => vec![sound],
				RadialButton::Folder(sound_folder) => sound_folder.sounds(),
			})
			.collect()
	}
}

#[derive(Clone)]
pub enum RadialButton {
	Sound(Sound),
	Folder(SoundFolder),
}

impl RadialButton {
	pub fn name(&self) -> &String {
		match self {
			RadialButton::Sound(sound) => &sound.name,
			RadialButton::Folder(sound_folder) => &sound_folder.name,
		}
	}

	pub fn icon(&self) -> Option<&String> {
		match self {
			RadialButton::Sound(sound) => sound.icon.as_ref(),
			RadialButton::Folder(sound_folder) => sound_folder.icon.as_ref(),
		}
	}
}

#[derive(Clone)]
pub struct Sound {
	pub name: String,
	pub icon: Option<String>,
	pub data: SoundData,
}

#[derive(Debug, Clone)]
pub struct SoundboardConfig {}
