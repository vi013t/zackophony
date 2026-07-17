use std::sync::LazyLock;

use derive_getters::Getters;
use eframe::egui::Color32;

#[derive(Getters)]
pub struct Theme {
	name: String,

	#[getter(copy)]
	sound_border: Color32,
	#[getter(copy)]
	sound_background: Color32,
	#[getter(copy)]
	sound_border_hover: Color32,
	#[getter(copy)]
	sound_background_hover: Color32,

	#[getter(copy)]
	sound_group_border: Color32,
	#[getter(copy)]
	sound_group_background: Color32,
	#[getter(copy)]
	sound_group_border_hover: Color32,
	#[getter(copy)]
	sound_group_background_hover: Color32,

	#[getter(copy)]
	background: Color32,
	#[getter(copy)]
	text_bright: Color32,
	#[getter(copy)]
	text: Color32,

	#[getter(copy)]
	red: Color32,
}

pub static CATPPUCCIN_MOCHA: LazyLock<Theme> = LazyLock::new(|| Theme {
	name: "Catppuccin Mocha".to_owned(),

	sound_border: Color32::from_hex("#b4befe").unwrap(),
	sound_background: Color32::from_hex("#11111bcc").unwrap(),
	sound_border_hover: Color32::from_hex("#f5c2e7").unwrap(),
	sound_background_hover: Color32::from_hex("#1e1e2e").unwrap(),

	sound_group_border: Color32::from_hex("#b5ebe2").unwrap(),
	sound_group_background: Color32::from_hex("#11111bcc").unwrap(),
	sound_group_border_hover: Color32::from_hex("#c8ebb5").unwrap(),
	sound_group_background_hover: Color32::from_hex("#1e1e2e").unwrap(),

	background: Color32::from_hex("#1e1e2e").unwrap(),
	text_bright: Color32::from_hex("#cdd6f4").unwrap(),
	text: Color32::from_hex("#a6adc8").unwrap(),

	red: Color32::from_hex("#f38ba8").unwrap(),
});
