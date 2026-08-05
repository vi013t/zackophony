use std::sync::LazyLock;

use derive_getters::Getters;
use eframe::egui::Color32;

#[derive(Getters)]
pub struct Theme {
	name: String,
	icon: Option<String>,

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
	background_dim: Color32,
	#[getter(copy)]
	background_dark: Color32,
	#[getter(copy)]
	background_bright: Color32,
	#[getter(copy)]
	text_bright: Color32,
	#[getter(copy)]
	text: Color32,
	#[getter(copy)]
	text_dim: Color32,

	#[getter(copy)]
	pink: Color32,
	#[getter(copy)]
	red: Color32,
	#[getter(copy)]
	green: Color32,
	#[getter(copy)]
	teal: Color32,

	#[getter(copy)]
	accent: Color32,
}

pub static CATPPUCCIN_LATTE: LazyLock<Theme> = LazyLock::new(|| Theme {
	name: "Latte".to_owned(),
	icon: Some(egui_phosphor::regular::SUN.to_owned()),

	sound_border: Color32::from_hex("#7287fd").unwrap(),
	sound_background: Color32::from_hex("#dce0e8cc").unwrap(),
	sound_border_hover: Color32::from_hex("#ea76cb").unwrap(),
	sound_background_hover: Color32::from_hex("#eff1f5").unwrap(),

	sound_group_border: Color32::from_hex("#04a5e5").unwrap(),
	sound_group_background: Color32::from_hex("#dce0e8cc").unwrap(),
	sound_group_border_hover: Color32::from_hex("#04e526").unwrap(),
	sound_group_background_hover: Color32::from_hex("#eff1f5").unwrap(),

	background: Color32::from_hex("#eff1f5").unwrap(),
	background_dim: Color32::from_hex("#e6e9ef").unwrap(),
	background_dark: Color32::from_hex("#dce0e8").unwrap(),
	background_bright: Color32::from_hex("#ccd0da").unwrap(),
	text_bright: Color32::from_hex("#4c4f69").unwrap(),
	text: Color32::from_hex("#6c6f85").unwrap(),
	text_dim: Color32::from_hex("#9ca0b0").unwrap(),

	pink: Color32::from_hex("#ea76cb").unwrap(),
	red: Color32::from_hex("#d20f39").unwrap(),
	green: Color32::from_hex("#40a02b").unwrap(),
	teal: Color32::from_hex("#179299").unwrap(),

	accent: Color32::from_hex("#7287fd").unwrap(),
});

pub static CATPPUCCIN_FRAPPE: LazyLock<Theme> = LazyLock::new(|| Theme {
	name: "Frappe".to_owned(),
	icon: Some(egui_phosphor::regular::CLOUD_SUN.to_owned()),

	sound_border: Color32::from_hex("#babbf1").unwrap(),
	sound_background: Color32::from_hex("#232634cc").unwrap(),
	sound_border_hover: Color32::from_hex("#f4b8e4").unwrap(),
	sound_background_hover: Color32::from_hex("#303446").unwrap(),

	sound_group_border: Color32::from_hex("#91d7e3").unwrap(),
	sound_group_background: Color32::from_hex("#232634cc").unwrap(),
	sound_group_border_hover: Color32::from_hex("#91e3ab").unwrap(),
	sound_group_background_hover: Color32::from_hex("#303446").unwrap(),

	background: Color32::from_hex("#303446").unwrap(),
	background_dim: Color32::from_hex("#292c3c").unwrap(),
	background_dark: Color32::from_hex("#232634").unwrap(),
	background_bright: Color32::from_hex("#414559").unwrap(),
	text_bright: Color32::from_hex("#c6d0f5").unwrap(),
	text: Color32::from_hex("#a5adce").unwrap(),
	text_dim: Color32::from_hex("#737994").unwrap(),

	pink: Color32::from_hex("#f4b8e4").unwrap(),
	red: Color32::from_hex("#e78284").unwrap(),
	green: Color32::from_hex("#a6d189").unwrap(),
	teal: Color32::from_hex("#81c8be").unwrap(),

	accent: Color32::from_hex("#babbf1").unwrap(),
});

pub static CATPPUCCIN_MACCHIATO: LazyLock<Theme> = LazyLock::new(|| Theme {
	name: "Macchiato".to_owned(),
	icon: Some(egui_phosphor::regular::CLOUD_MOON.to_owned()),

	sound_border: Color32::from_hex("#b7bdf8").unwrap(),
	sound_background: Color32::from_hex("#181926cc").unwrap(),
	sound_border_hover: Color32::from_hex("#f5bde6").unwrap(),
	sound_background_hover: Color32::from_hex("#24273a").unwrap(),

	sound_group_border: Color32::from_hex("#91d7e3").unwrap(),
	sound_group_background: Color32::from_hex("#181926cc").unwrap(),
	sound_group_border_hover: Color32::from_hex("#91e3ab").unwrap(),
	sound_group_background_hover: Color32::from_hex("#24273a").unwrap(),

	background: Color32::from_hex("#24273a").unwrap(),
	background_dim: Color32::from_hex("#1e2030").unwrap(),
	background_dark: Color32::from_hex("#181926").unwrap(),
	background_bright: Color32::from_hex("#363a4f").unwrap(),
	text_bright: Color32::from_hex("#cad3f5").unwrap(),
	text: Color32::from_hex("#a5adcb").unwrap(),
	text_dim: Color32::from_hex("#6e738d").unwrap(),

	pink: Color32::from_hex("#f4b8e4").unwrap(),
	red: Color32::from_hex("#ed8796").unwrap(),
	green: Color32::from_hex("#a6da95").unwrap(),
	teal: Color32::from_hex("#8bd5ca").unwrap(),

	accent: Color32::from_hex("#b7bdf8").unwrap(),
});

pub static CATPPUCCIN_MOCHA: LazyLock<Theme> = LazyLock::new(|| Theme {
	name: "Mocha".to_owned(),
	icon: Some(egui_phosphor::regular::MOON_STARS.to_owned()),

	sound_border: Color32::from_hex("#b4befe").unwrap(),
	sound_background: Color32::from_hex("#11111bcc").unwrap(),
	sound_border_hover: Color32::from_hex("#f5c2e7").unwrap(),
	sound_background_hover: Color32::from_hex("#1e1e2e").unwrap(),

	sound_group_border: Color32::from_hex("#b5ebe2").unwrap(),
	sound_group_background: Color32::from_hex("#11111bcc").unwrap(),
	sound_group_border_hover: Color32::from_hex("#c8ebb5").unwrap(),
	sound_group_background_hover: Color32::from_hex("#1e1e2e").unwrap(),

	background: Color32::from_hex("#1e1e2e").unwrap(),
	background_dim: Color32::from_hex("#181825").unwrap(),
	background_dark: Color32::from_hex("#11111b").unwrap(),
	background_bright: Color32::from_hex("#313244").unwrap(),
	text_bright: Color32::from_hex("#cdd6f4").unwrap(),
	text: Color32::from_hex("#a6adc8").unwrap(),
	text_dim: Color32::from_hex("#6c7086").unwrap(),

	pink: Color32::from_hex("#f5bde6").unwrap(),
	red: Color32::from_hex("#f38ba8").unwrap(),
	green: Color32::from_hex("#a6e3a1").unwrap(),
	teal: Color32::from_hex("#94e2d5").unwrap(),

	accent: Color32::from_hex("#b4befe").unwrap(),
});

pub static THEMES: &[&LazyLock<Theme>] = &[&CATPPUCCIN_LATTE, &CATPPUCCIN_FRAPPE, &CATPPUCCIN_MACCHIATO, &CATPPUCCIN_MOCHA];
