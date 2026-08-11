use std::{
	collections::{HashMap, HashSet},
	f32::consts::TAU,
	ops::RangeInclusive,
	sync::{
		Arc,
		Mutex,
		atomic::{AtomicBool, Ordering},
		mpmc::{Receiver, channel},
	},
};

use eframe::{
	Frame,
	Renderer,
	egui::{
		self,
		Align,
		Align2,
		Area,
		Button,
		Color32,
		ComboBox,
		CursorIcon,
		Event,
		FontDefinitions,
		Key,
		Label,
		Layout,
		Margin,
		Modifiers,
		Order,
		Painter,
		Pos2,
		Rect,
		RichText,
		Shadow,
		Shape,
		Slider,
		Stroke,
		TextEdit,
		TextWrapMode,
		Ui,
		UiBuilder,
		Vec2,
		ViewportBuilder,
		ViewportCommand,
		Visuals,
		style::{HandleShape, StyleModifier},
		vec2,
	},
	emath::{self, Rot2},
};
use egui_div::{Gradient, GradientDirection, Listeners, button, div};
use egui_phosphor::regular as icons;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use rand::{rngs::ThreadRng, seq::IndexedRandom as _};
use rodio::DeviceTrait;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use uuid::Uuid;

use crate::{
	NAME,
	VERSION,
	audio::{AudioManager, SoundData, UiSound},
	keybindings::{ArgumentType, HotkeyBehavior, Keybinding, KeybindingArgument, KeybindingArgumentValue, LocalKeybinding},
	soundboard::{Sound, SoundGroup, SoundPage, SoundTreeNode, SoundboardConfig},
	ui::theme::{THEMES, Theme},
};

pub mod theme;

#[derive(Debug, Clone, PartialEq, Eq, Copy, Hash)]
enum SettingsTab {
	Appearance,
	Hotkeys,
	System,
	Sounds(Option<Uuid>),
	Support,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Hash)]
enum AppScreen {
	Soundboard,
	Settings(SettingsTab),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, EnumIter)]
pub enum UiAction {
	ToggleSoundboard,
	OpenSoundboard,
	CloseSoundboard,
	PlaySound,
}

impl UiAction {
	pub fn arguments(&self) -> Vec<KeybindingArgument> {
		match self {
			UiAction::PlaySound => vec![KeybindingArgument {
				name: "Sound",
				argument_type: ArgumentType::Sound,
			}],
			_ => Vec::new(),
		}
	}

	pub fn force_global(&self) -> bool {
		matches!(self, Self::ToggleSoundboard | Self::OpenSoundboard)
	}

	pub fn force_local(&self) -> bool {
		matches!(self, Self::CloseSoundboard)
	}

	pub fn name(&self) -> &'static str {
		match self {
			UiAction::ToggleSoundboard => "Toggle soundboard",
			UiAction::OpenSoundboard => "Open soundboard",
			UiAction::CloseSoundboard => "Close soundboard",
			UiAction::PlaySound => "Play sound",
		}
	}

	pub fn from_name(name: &str) -> Option<UiAction> {
		match name {
			"Toggle soundboard" => Some(UiAction::ToggleSoundboard),
			"Open soundboard" => Some(UiAction::OpenSoundboard),
			"Close soundboard" => Some(UiAction::CloseSoundboard),
			"Play sound" => Some(UiAction::PlaySound),
			_ => None,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
#[allow(clippy::enum_variant_names, reason = "might have non-delete popups later")]
enum Popup {
	DeleteSound,
	DeletePage,
	DeleteGroup,
}

pub struct App {
	visible: bool,
	screen: AppScreen,

	hotkey_manager: GlobalHotKeyManager,
	rx: Receiver<UiAction>,
	rng: ThreadRng,

	audio_player: Arc<Mutex<AudioManager>>,
	config: SoundboardConfig,
	cute_message: &'static str,

	keybinding_arguments: Arc<Mutex<Option<HashMap<String, KeybindingArgumentValue>>>>,

	popup: Option<Popup>,
	recording_keybinding: Option<Uuid>,
	is_recording: Arc<AtomicBool>,
	settings_sound_page: usize,
	icon_picker_coords: Option<Pos2>,
	icon_picker_search_text: String,

	hovered_path: Option<Vec<usize>>,
	active_path: Vec<usize>,
	last_hovered_path: Option<Vec<usize>>,
	right_clicked_id: Option<Uuid>,
	picking_file: Option<Option<Uuid>>,
	page: usize,

	open_groups: HashSet<Uuid>,
}

impl App {
	fn new(rx: Receiver<UiAction>) -> anyhow::Result<App> {
		let config = SoundboardConfig::default();
		let mut audio_player = AudioManager::new(
			config.input_device.clone().unwrap(),
			config.output_device.clone().unwrap(),
			config.virtual_cable_device.clone().unwrap(),
		)?;
		for sound in config.soundboard.sounds() {
			audio_player.upload_user_sound(&sound.name, sound.data.clone())?;
		}

		let hotkey_manager = GlobalHotKeyManager::new()?;
		for keybinding in config.keybindings.lock().unwrap().iter() {
			if keybinding.global
				&& let Some(hotkey) = keybinding.key.to_global()
			{
				hotkey_manager.register(hotkey)?;
			}
		}

		let mut rng = rand::rng();

		Ok(App {
			hotkey_manager,
			screen: AppScreen::Soundboard,
			config,
			audio_player: Arc::new(Mutex::new(audio_player)),
			cute_message: random_cute_message(&mut rng),
			rng,
			rx,
			visible: false,
			popup: None,
			recording_keybinding: None,
			icon_picker_search_text: String::new(),
			icon_picker_coords: None,
			settings_sound_page: 0,
			is_recording: Arc::new(AtomicBool::new(false)),
			last_hovered_path: None,
			picking_file: None,
			keybinding_arguments: Arc::new(Mutex::new(None)),
			hovered_path: None,
			active_path: Vec::new(),
			right_clicked_id: None,
			open_groups: HashSet::new(),
			page: 0,
		})
	}

	fn show(&mut self, ui: &mut Ui) {
		self.visible = true;
		ui.send_viewport_cmd(ViewportCommand::MousePassthrough(false));
		ui.send_viewport_cmd(ViewportCommand::Focus);
	}

	fn hide(&mut self, ui: &mut Ui) {
		self.visible = false;
		self.screen = AppScreen::Soundboard;
		ui.send_viewport_cmd(ViewportCommand::MousePassthrough(true));

		// clear popups or state caches
		self.hovered_path = None;
		self.active_path = Vec::new();
		self.right_clicked_id = None;
		self.popup = None;
		self.icon_picker_coords = None;

		// generate a new cute message :D
		let mut cute_message = random_cute_message(&mut self.rng);
		while cute_message == self.cute_message {
			cute_message = random_cute_message(&mut self.rng);
		}
		self.cute_message = cute_message;

		// stop any keybinding recording
		self.is_recording.store(false, Ordering::Relaxed);
		self.recording_keybinding = None;

		// save preferences to disk
		let _ = self.config.save_to_disk();
	}

	pub fn start() -> anyhow::Result<()> {
		let (tx, rx) = channel();
		let app = App::new(rx)?;

		let options = eframe::NativeOptions {
			viewport: ViewportBuilder::default()
				.with_title(NAME)
				.with_transparent(true)
				.with_decorations(false)
				.with_always_on_top()
				.with_visible(false)
				.with_taskbar(false)
				.with_maximized(true),
			renderer: Renderer::Glow,
			..Default::default()
		};

		eframe::run_native(
			NAME,
			options,
			Box::new(move |cc| {
				let ctx = cc.egui_ctx.clone();

				// load icon font
				let mut fonts = FontDefinitions::default();
				egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
				cc.egui_ctx.set_fonts(fonts);

				let is_recording = app.is_recording.clone();
				let keybindings = app.config.keybindings.clone();
				let keybinding_arguments = app.keybinding_arguments.clone();

				std::thread::spawn(move || {
					let receiver = GlobalHotKeyEvent::receiver();
					let _ = tx.send(UiAction::CloseSoundboard);
					loop {
						if let Ok(event) = receiver.recv() {
							if is_recording.load(Ordering::Relaxed) {
								continue;
							}

							for keybinding in keybindings.lock().unwrap().iter() {
								if keybinding.global
									&& let Some(hotkey) = keybinding.key.to_global()
									&& hotkey.id() == event.id()
								{
									*keybinding_arguments.lock().unwrap() = Some(keybinding.arguments.clone());
									match keybinding.behavior {
										HotkeyBehavior::Press => {
											if event.state == HotKeyState::Pressed {
												let _ = tx.send(keybinding.action);
											}
										},
										HotkeyBehavior::Release => {
											if event.state == HotKeyState::Released {
												let _ = tx.send(keybinding.action);
											}
										},
									}
									ctx.request_repaint();
								}
							}
						}
					}
				});

				Ok(Box::new(app))
			}),
		)
		.unwrap();

		Ok(())
	}

	fn point_in_polygon(pos: Pos2, polygon: &[Pos2]) -> bool {
		let mut inside = false;
		let mut j = polygon.len() - 1;
		for i in 0..polygon.len() {
			let pi = polygon[i];
			let pj = polygon[j];
			if (pi.y > pos.y) != (pj.y > pos.y) && pos.x < (pj.x - pi.x) * (pos.y - pi.y) / (pj.y - pi.y) + pi.x {
				inside = !inside;
			}
			j = i;
		}
		inside
	}

	fn get_wedge_polygon(screen_center: Pos2, inner_radius: f32, outer_radius: f32, button_spacing: f32, slice_angle: f32, center_angle: f32) -> Vec<Pos2> {
		let outer_gap_angle = button_spacing / outer_radius.max(1.0);
		let inner_gap_angle = button_spacing / inner_radius.max(1.0);

		let start_angle_outer = center_angle - (slice_angle / 2.0) + outer_gap_angle;
		let end_angle_outer = center_angle + (slice_angle / 2.0) - outer_gap_angle;

		let start_angle_inner = center_angle - (slice_angle / 2.0) + inner_gap_angle;
		let end_angle_inner = center_angle + (slice_angle / 2.0) - inner_gap_angle;

		let mut points = Vec::new();
		let num_arc_points = 16;

		for step in 0..=num_arc_points {
			let t = step as f32 / num_arc_points as f32;
			let a = start_angle_outer + (end_angle_outer - start_angle_outer) * t;
			points.push(screen_center + Vec2::new(a.cos(), a.sin()) * outer_radius);
		}

		for step in (0..=num_arc_points).rev() {
			let t = step as f32 / num_arc_points as f32;
			let a = start_angle_inner + (end_angle_inner - start_angle_inner) * t;
			points.push(screen_center + Vec2::new(a.cos(), a.sin()) * inner_radius);
		}

		points
	}

	fn draw_debug_info(&mut self, ui: &mut Ui) {
		egui::Area::new("debug-info".into())
			.order(Order::Foreground)
			.anchor(egui::Align2::LEFT_TOP, Vec2::ZERO)
			.show(ui, |ui| {
				eframe::egui::Frame::NONE
					.fill(self.config.theme.background())
					.corner_radius(8.)
					.inner_margin(8.)
					.outer_margin(8.)
					.show(ui, |ui| {
						ui.vertical(|ui| {
							ui.label(RichText::new(format!("{}   {NAME} v{VERSION}", icons::PAW_PRINT)).color(self.config.theme.accent()));
							ui.add_space(2.);
							ui.add(Label::new(RichText::new(self.cute_message).color(self.config.theme.text())).wrap_mode(TextWrapMode::Extend));
						});
					});
			});
	}

	fn draw_config_buttons(&mut self, ui: &mut Ui) {
		egui::Area::new("bottom_right_panel".into())
			.order(Order::Foreground)
			.anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-10.0, -10.0))
			.show(ui, |ui| {
				ui.vertical_centered(|ui| {
					button()
						.size([50., 50.])
						.corner_radius(25.)
						.text(icons::GEAR)
						.text_color(self.config.theme.accent())
						.corner_radius(25.)
						.font_size(28.)
						.fill(self.config.theme.sound_background())
						.stroke(Stroke::NONE)
						.add_to(ui)
						.on_click(|| self.screen = AppScreen::Settings(SettingsTab::Appearance));
				});
			});

		egui::Area::new("top-right-buttons".into())
			.order(Order::Foreground)
			.anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
			.show(ui, |ui| {
				ui.vertical_centered(|ui| {
					button()
						.size([40., 40.])
						.corner_radius(20.)
						.text(icons::X)
						.text_color(self.config.theme.red())
						.corner_radius(25.)
						.font_size(16.)
						.fill(self.config.theme.sound_background())
						.stroke(Stroke::NONE)
						.add_to(ui)
						.on_click(|| self.hide(ui));
				});
			});
	}

	pub fn draw_delete_page_popup(&mut self, ui: &mut Ui) {
		let parent = ui.viewport_rect();
		let size = vec2(300., 250.);
		let child = Rect::from_center_size(parent.center(), size);
		ui.scope_builder(UiBuilder::new().max_rect(parent), |ui| {
			empty_frame().fill(Color32::TRANSPARENT).show(ui, |ui| {
				ui.scope_builder(UiBuilder::new().max_rect(child), |ui| {
					empty_frame()
						.fill(self.config.theme.background())
						.inner_margin(16.)
						.corner_radius(8.)
						.stroke(Stroke::new(1., self.config.theme.background_bright()))
						.shadow(Shadow {
							offset: [0, 0],
							blur: 8,
							spread: 2,
							color: Color32::BLACK,
						})
						.show(ui, |ui| {
							ui.vertical_centered_justified(|ui| {
								ui.label(
									RichText::new("Are you sure you want to delete this page? All sounds and groups inside will be deleted. This can't be undone.")
										.size(16.)
										.color(self.config.theme.text()),
								);
								ui.add_space(16.);
								ui.columns(2, |ui| {
									let cancel_button = ui[0].add_sized(
										[100., 28.],
										Button::new(RichText::new("Cancel").size(16.).color(self.config.theme.background_dark()))
											.fill(self.config.theme.red())
											.corner_radius(4.),
									);

									if cancel_button.hovered() {
										ui[0].set_cursor_icon(egui::CursorIcon::PointingHand);
									}

									if cancel_button.clicked() {
										self.popup = None;
									}

									let confirm_button = ui[1].add_sized(
										[100., 28.],
										Button::new(RichText::new("Confirm").size(16.).color(self.config.theme.background_dark()))
											.fill(self.config.theme.green())
											.corner_radius(4.),
									);

									if confirm_button.hovered() {
										ui[1].set_cursor_icon(egui::CursorIcon::PointingHand);
									}

									if confirm_button.clicked() {
										if let AppScreen::Settings(SettingsTab::Sounds(_)) = &self.screen {
											self.config.soundboard.pages.remove(self.settings_sound_page);
											if self.settings_sound_page == self.config.soundboard.pages.len() {
												self.settings_sound_page -= 1;
											}
											let _ = self.config.save_to_disk();
											ui[1].request_repaint();
										}

										self.popup = None;
										self.screen = match &self.screen {
											AppScreen::Settings(SettingsTab::Sounds(Some(_))) => AppScreen::Settings(SettingsTab::Sounds(None)),
											_ => self.screen,
										};
									}
								});
							});
						})
				});
			});
		});
	}

	pub fn draw_delete_group_popup(&mut self, ui: &mut Ui) {
		let parent = ui.viewport_rect();
		let size = vec2(300., 130.);
		let child = Rect::from_center_size(parent.center(), size);
		ui.scope_builder(UiBuilder::new().max_rect(parent), |ui| {
			empty_frame().fill(Color32::TRANSPARENT).show(ui, |ui| {
				ui.scope_builder(UiBuilder::new().max_rect(child), |ui| {
					div()
						.size(size)
						.fill(self.config.theme.background())
						.inner_margin(16.)
						.corner_radius(8.)
						.stroke(Stroke::new(1., self.config.theme.background_bright()))
						.shadow(Shadow {
							offset: [0, 0],
							blur: 8,
							spread: 2,
							color: Color32::BLACK,
						})
						.show(ui, |ui| {
							ui.vertical_centered_justified(|ui| {
								ui.label(
									RichText::new("Are you sure you want to delete this group? All sounds and groups inside will be deleted. This can't be undone.")
										.size(14.)
										.color(self.config.theme.text()),
								);
								ui.add_space(16.);
								ui.columns(2, |ui| {
									button()
										.size([125., 28.])
										.text("Cancel")
										.font_size(14.)
										.text_color(self.config.theme.background_dark())
										.fill(
											Gradient::new()
												.color1(self.config.theme.pink())
												.color2(self.config.theme.red())
												.direction(GradientDirection::ToBottomRight),
										)
										.add_to(&mut ui[0])
										.on_click(|| self.popup = None);

									button()
										.size([125., 28.])
										.text("Confirm")
										.font_size(14.)
										.text_color(self.config.theme.background_dark())
										.fill(
											Gradient::new()
												.color1(self.config.theme.teal())
												.color2(self.config.theme.green())
												.direction(GradientDirection::ToBottomRight),
										)
										.add_to(&mut ui[1])
										.on_click(|| {
											if let AppScreen::Settings(SettingsTab::Sounds(Some(id))) = &self.screen {
												self.config.soundboard.remove_sound_node(*id);
												let _ = self.config.save_to_disk();
											}

											self.popup = None;
											self.screen = match &self.screen {
												AppScreen::Settings(SettingsTab::Sounds(Some(_))) => AppScreen::Settings(SettingsTab::Sounds(None)),
												_ => self.screen,
											};
										});
								});
							});
						})
				});
			});
		});
	}

	pub fn draw_delete_sound_popup(&mut self, ui: &mut Ui) {
		let parent = ui.viewport_rect();
		let size = vec2(300., 130.);
		let child = Rect::from_center_size(parent.center(), size);
		ui.scope_builder(UiBuilder::new().max_rect(parent), |ui| {
			empty_frame().fill(Color32::TRANSPARENT).show(ui, |ui| {
				ui.scope_builder(UiBuilder::new().max_rect(child), |ui| {
					div()
						.size(size)
						.fill(self.config.theme.background())
						.inner_margin(16.)
						.corner_radius(8.)
						.stroke(Stroke::new(1., self.config.theme.background_bright()))
						.shadow(Shadow {
							offset: [0, 0],
							blur: 8,
							spread: 2,
							color: Color32::BLACK,
						})
						.show(ui, |ui| {
							ui.vertical_centered_justified(|ui| {
								ui.label(
									RichText::new("Are you sure you want to delete this sound? This can't be undone.")
										.size(16.)
										.color(self.config.theme.text()),
								);
								ui.add_space(16.);
								ui.columns(2, |ui| {
									button()
										.size([125., 28.])
										.text("Cancel")
										.font_size(14.)
										.text_color(self.config.theme.background_dark())
										.fill(
											Gradient::new()
												.color1(self.config.theme.pink())
												.color2(self.config.theme.red())
												.direction(GradientDirection::ToBottomRight),
										)
										.add_to(&mut ui[0])
										.on_click(|| self.popup = None);

									button()
										.size([125., 28.])
										.text("Confirm")
										.font_size(14.)
										.text_color(self.config.theme.background_dark())
										.fill(
											Gradient::new()
												.color1(self.config.theme.teal())
												.color2(self.config.theme.green())
												.direction(GradientDirection::ToBottomRight),
										)
										.add_to(&mut ui[1])
										.on_click(|| {
											match &self.screen {
												AppScreen::Settings(SettingsTab::Sounds(Some(sound_id))) => {
													self.config.soundboard.remove_sound_node(*sound_id);
													let _ = self.config.save_to_disk();
												},
												AppScreen::Soundboard => {
													if let Some(id) = self.right_clicked_id {
														self.config.soundboard.remove_sound_node(id);
														let _ = self.config.save_to_disk();
													}
												},
												_ => {},
											}

											self.popup = None;
											self.screen = match &self.screen {
												AppScreen::Settings(SettingsTab::Sounds(Some(_))) => AppScreen::Settings(SettingsTab::Sounds(None)),
												_ => self.screen,
											};
										});
								});
							});
						})
				});
			});
		});
	}

	fn draw_menu(&mut self, ui: &mut Ui) {
		let num_items = self.config.soundboard.pages[self.page].sounds.len();
		if num_items == 0 {
			return;
		}

		let screen_center = ui.ctx().viewport_rect().center();
		let max_radius = 9999999999.;

		let mut sound_to_play: Option<Sound> = None;
		let mut should_hide = false;

		egui::Area::new("radial_menu_layer".into()).show(ui.ctx(), |ui| {
			let area_rect = Rect::from_center_size(screen_center, Vec2::splat(max_radius * 2.2));
			let response = ui.allocate_rect(area_rect, egui::Sense::click_and_drag());
			let painter = ui.painter_at(area_rect);

			let mut rings = Vec::new();
			let root_nodes = &self.config.soundboard.pages[self.page].sounds;
			rings.push((0, root_nodes, 0.0, TAU / root_nodes.len().max(1) as f32, Vec::new()));

			let mut current_nodes = root_nodes;
			let mut current_parent_angle = 0.0;
			let mut current_path = Vec::new();

			for (level, &idx) in self.active_path.iter().enumerate() {
				let slice_angle = if level == 0 { TAU / current_nodes.len().max(1) as f32 } else { 0.7 };

				let center_angle = if level == 0 {
					idx as f32 * slice_angle
				} else {
					let offset = (current_nodes.len() as f32 - 1.0) / 2.0 * 0.7;
					current_parent_angle + idx as f32 * 0.7 - offset
				};

				if let Some(SoundTreeNode::Group(group)) = current_nodes.get(idx) {
					current_nodes = &group.sounds;
					current_parent_angle = center_angle;
					current_path.push(idx);
					rings.push((level + 1, current_nodes, current_parent_angle, 0.7, current_path.clone()));
				} else {
					break;
				}
			}

			let mut new_hovered_path = None;
			let mut new_active_path = self.active_path.clone();

			if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
				for (level, nodes, parent_angle, slice_angle, path_prefix) in rings.iter().rev() {
					let (inner_r, outer_r) = App::get_ring_radii(*level);
					let button_spacing = 8.0;

					for (idx, _node) in nodes.iter().enumerate() {
						let center_angle = if *level == 0 {
							idx as f32 * slice_angle
						} else {
							let offset = (nodes.len() as f32 - 1.0) / 2.0 * slice_angle;
							parent_angle + idx as f32 * slice_angle - offset
						};

						let points = Self::get_wedge_polygon(screen_center, inner_r, outer_r, button_spacing, *slice_angle, center_angle);

						if Self::point_in_polygon(pointer_pos, &points) {
							let mut full_path = path_prefix.clone();
							full_path.push(idx);
							new_hovered_path = Some(full_path);
							break;
						}
					}
					if new_hovered_path.is_some() {
						break;
					}
				}

				if let Some(path) = &new_hovered_path {
					let mut current = &self.config.soundboard.pages[self.page].sounds;
					let mut is_group = false;
					for (i, &p) in path.iter().enumerate() {
						if let Some(node) = current.get(p) {
							if i == path.len() - 1 {
								is_group = matches!(node, SoundTreeNode::Group(_));
							}
							if let SoundTreeNode::Group(g) = node {
								current = &g.sounds;
							}
						}
					}

					if is_group {
						new_active_path = path.clone();
					} else {
						let mut parent_path = path.clone();
						parent_path.pop();
						new_active_path = parent_path;
					}
				}
			}

			if response.clicked()
				&& let Some(path) = &new_hovered_path
			{
				let mut current = &self.config.soundboard.pages[self.page].sounds;
				let mut clicked_sound = None;
				for (i, &p) in path.iter().enumerate() {
					if let Some(node) = current.get(p) {
						if i == path.len() - 1
							&& let SoundTreeNode::Sound(sound) = node
						{
							clicked_sound = Some(sound.clone());
						}
						if let SoundTreeNode::Group(g) = node {
							current = &g.sounds;
						}
					}
				}

				if let Some(sound) = clicked_sound {
					sound_to_play = Some(sound);
					should_hide = true;
				}
			}

			if response.secondary_clicked() {
				if let Some(path) = &new_hovered_path {
					let mut current = &self.config.soundboard.pages[self.page].sounds;
					let mut right_clicked = None;
					for (i, &p) in path.iter().enumerate() {
						if let Some(node) = current.get(p) {
							if i == path.len() - 1 {
								right_clicked = Some(node.id());
							}
							if let SoundTreeNode::Group(g) = node {
								current = &g.sounds;
							}
						}
					}
					self.right_clicked_id = right_clicked;
				} else {
					self.right_clicked_id = None;
				}
			}

			self.hovered_path = new_hovered_path;
			self.active_path = new_active_path;

			if self.hovered_path.is_some() {
				ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
				if self.hovered_path != self.last_hovered_path {
					let _ = self.audio_player.lock().unwrap().play_ui_sound(UiSound::Hover, self.config.ui_volume);
				}
			}
			self.last_hovered_path = self.hovered_path.clone();

			if let Some(id) = self.right_clicked_id {
				response.context_menu(|ui| {
					let visuals = ui.visuals_mut();
					visuals.window_fill = self.config.theme.sound_background();
					visuals.window_stroke = Stroke::new(1., self.config.theme.sound_border());

					if let Some(sound_node) = self.config.soundboard.get_sound_node(id) {
						let (name, icon) = match sound_node {
							SoundTreeNode::Sound(sound) => (&sound.name, &sound.icon),
							SoundTreeNode::Group(folder) => (&folder.name, &folder.icon),
						};

						if let Some(icon) = icon {
							ui.label(RichText::new(format!(" {}   {}", icon, name)).color(Color32::from_hex("#cdd6f4").unwrap()));
						} else {
							ui.label(format!(" {}", name));
						}

						ui.separator();

						let rename_button = ui.add(
							Button::new(RichText::new(format!(" {}   Rename", icons::TEXTBOX)).color(self.config.theme.text()))
								.min_size(Vec2::new(200., 25.))
								.fill(Color32::from_hex("#11111b").unwrap()),
						);
						if rename_button.hovered() {
							ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
						}
						if rename_button.clicked() {
							self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(id)));
						}

						let change_icon_button = ui.add(
							Button::new(RichText::new(format!(" {}   Change icon", icons::DIAMONDS_FOUR)).color(self.config.theme.text()))
								.min_size(Vec2::new(200., 25.))
								.fill(Color32::from_hex("#11111b").unwrap()),
						);
						if change_icon_button.hovered() {
							ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
						}
						if change_icon_button.clicked() {
							self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(id)));
						}

						if matches!(sound_node, SoundTreeNode::Sound(_)) {
							let adjust_volume_for_me_button = ui.add(
								Button::new(RichText::new(format!(" {}   Adjust volume for me", icons::SPEAKER_HIGH)).color(self.config.theme.text()))
									.min_size(Vec2::new(200., 25.))
									.fill(Color32::from_hex("#11111b").unwrap()),
							);
							if adjust_volume_for_me_button.hovered() {
								ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
							}
							if adjust_volume_for_me_button.clicked() {
								self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(id)));
							}

							let adjust_volume_for_others_button = ui.add(
								Button::new(RichText::new(format!(" {}   Adjust volume for others", icons::SPEAKER_HIGH)).color(self.config.theme.text()))
									.min_size(Vec2::new(200., 25.))
									.fill(Color32::from_hex("#11111b").unwrap()),
							);
							if adjust_volume_for_others_button.hovered() {
								ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
							}
							if adjust_volume_for_others_button.clicked() {
								self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(id)));
							}
						} else {
							let add_sound_button = ui.add(
								Button::new(RichText::new(format!(" {}   Add sound", icons::SPEAKER_HIGH)).color(self.config.theme.text()))
									.min_size(Vec2::new(200., 25.))
									.fill(Color32::from_hex("#11111b").unwrap()),
							);
							if add_sound_button.hovered() {
								ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
							}
							if add_sound_button.clicked() {
								self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(id)));
							};

							let add_group_button = ui.add(
								Button::new(RichText::new(format!(" {}   Add group", icons::FOLDER)).color(self.config.theme.text()))
									.min_size(Vec2::new(200., 25.))
									.fill(Color32::from_hex("#11111b").unwrap()),
							);
							if add_group_button.hovered() {
								ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
							}
							if add_group_button.clicked() {
								self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(id)));
							};
						};

						// let move_to_button = ui.add(
						// 	Button::new(RichText::new(format!(" {}   Move to...", icons::FOLDER)).color(self.config.theme.text()))
						// 		.min_size(Vec2::new(200., 25.))
						// 		.fill(Color32::from_hex("#11111b").unwrap()),
						// );
						// if move_to_button.hovered() {
						// 	ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
						// }
						// if move_to_button.clicked() {
						// 	self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(id)));
						// }

						ui.separator();

						let delete_button = ui.add(
							Button::new(RichText::new(format!(" {}   Delete", icons::TRASH)).color(self.config.theme.red()))
								.min_size(Vec2::new(200., 25.))
								.fill(Color32::from_hex("#11111b").unwrap()),
						);
						if delete_button.hovered() {
							ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
						}
						if delete_button.clicked() {
							self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(id)));
						}
					}
				});
			}

			for (level, nodes, parent_angle, slice_angle, path_prefix) in &rings {
				let (inner_r, outer_r) = App::get_ring_radii(*level);
				let button_spacing = 8.0;

				for (idx, node) in nodes.iter().enumerate() {
					let center_angle = if *level == 0 {
						idx as f32 * slice_angle
					} else {
						let offset = (nodes.len() as f32 - 1.0) / 2.0 * slice_angle;
						parent_angle + idx as f32 * slice_angle - offset
					};

					let mut full_path = path_prefix.clone();
					full_path.push(idx);

					self.draw_radial_button(ui, &painter, node, inner_r, outer_r, button_spacing, &full_path, *slice_angle, center_angle);

					if matches!(node, SoundTreeNode::Group(_)) {
						let is_expanded = self.active_path.starts_with(&full_path);
						if !is_expanded {
							let angle = center_angle;
							let radius = outer_r + 15.0;
							let x = radius * angle.cos();
							let y = radius * angle.sin();

							let origin = screen_center + vec2(x, y);
							let size = 15.0;
							let points = [origin, Pos2::new(origin.x + size, origin.y), Pos2::new(origin.x, origin.y + size)];
							let rotated = rotate_triangle_around_centroid(points, angle + 3.0 / 8.0 * TAU);
							painter.add(Shape::convex_polygon(rotated, self.config.theme.sound_group_border(), Stroke::NONE));
						}
					}
				}
			}
		});

		if let Some(sound_name) = sound_to_play {
			let _ = self.audio_player.lock().unwrap().play_user_sound(&sound_name);
		}
		if should_hide {
			self.hide(ui);
		}
	}

	#[allow(clippy::too_many_arguments, reason = "shrug lgtm")]
	fn draw_radial_button(
		&self,
		ui: &mut Ui,
		painter: &Painter,
		sound_node: &SoundTreeNode,
		inner_radius: f32,
		outer_radius: f32,
		button_spacing: f32,
		path: &[usize],
		slice_angle: f32,
		center_angle: f32,
	) {
		let is_hovered = self.hovered_path.as_deref() == Some(path);

		let stroke_width = if is_hovered { 5.0 } else { 2.0 };
		let gap = if is_hovered { 14. } else { 12. };
		let hover_expand = 10.0;

		let fill_color = sound_node.background_color(self.config.theme, is_hovered);
		let stroke_color = sound_node.border_color(self.config.theme, is_hovered);
		let text_color = stroke_color;

		// Path works naturally with egui's hashing logic to maintain specific IDs
		let hover_t = ui.ctx().animate_bool(ui.id().with(path), is_hovered);
		let current_outer_radius = outer_radius + (hover_expand * hover_t);
		let current_inner_radius = inner_radius + (3.0 * hover_t);

		let screen_center = ui.ctx().viewport_rect().center();
		let points = Self::get_wedge_polygon(screen_center, current_inner_radius, current_outer_radius, button_spacing, slice_angle, center_angle);

		self.draw_shadow(painter, &points);
		let fill_shape = Shape::convex_polygon(points.clone(), fill_color, Stroke::NONE);
		painter.add(fill_shape);
		painter.add(Shape::closed_line(points, Stroke::new(stroke_width, stroke_color)));

		let mid_radius = (current_inner_radius + current_outer_radius) / 2.0;
		let label_pos = screen_center + Vec2::new(center_angle.cos(), center_angle.sin()) * mid_radius;

		if let Some(icon) = sound_node.icon() {
			painter.text(
				label_pos - Vec2::new(0., gap),
				egui::Align2::CENTER_CENTER,
				icon,
				egui::FontId::proportional(28.0 + (2.0 * hover_t)),
				text_color,
			);
			painter.text(
				label_pos + Vec2::new(0., gap),
				egui::Align2::CENTER_CENTER,
				sound_node.name(),
				egui::FontId::proportional(12.0 + (2.0 * hover_t)),
				text_color,
			);
		}
	}

	fn draw_shadow(&self, painter: &Painter, points: &[Pos2]) {
		let layers = [(22.0, 2), (18.0, 3), (14.0, 5), (11.0, 7), (8.0, 10), (6.0, 14), (4.0, 18)];

		for (width, alpha) in layers {
			painter.add(Shape::closed_line(
				points.to_vec(),
				Stroke::new(width, Color32::from_rgba_unmultiplied(0, 0, 0, alpha + 10)),
			));
		}
	}

	fn draw_screen(&mut self, ui: &mut Ui, overlay_frame: eframe::egui::Frame) {
		match self.screen {
			AppScreen::Soundboard => self.draw_soundboard(ui, overlay_frame),
			AppScreen::Settings(tab) => self.draw_settings(ui, overlay_frame, tab),
		}
	}

	fn draw_settings(&mut self, ui: &mut Ui, overlay_frame: eframe::egui::Frame, tab: SettingsTab) {
		egui::CentralPanel::default().frame(overlay_frame).show(ui, |ui| {
			let parent = ui.max_rect();
			let size = parent.size() * 0.55;
			let child = Rect::from_center_size(parent.center(), size);

			ui.scope_builder(UiBuilder::new().max_rect(child), |ui| {
				div()
					.fill(
						Gradient::new()
							.color1(self.config.theme.background())
							.color2(self.config.theme.background_dim())
							.direction(GradientDirection::ToBottomRight),
					)
					.size(child.size())
					.inner_margin(16.)
					.corner_radius(8.)
					.stroke(Stroke::new(1., self.config.theme.background_bright()))
					.shadow(Shadow::default())
					.show(ui, |ui| {
						ui.horizontal(|ui| {
							div()
								.size([200., 28.])
								.text(format!(" {}   {NAME} Settings", icons::GEAR))
								.font_size(16.)
								.text_color(self.config.theme.text_bright())
								.add_to(ui);

							ui.add_space(ui.available_width() - 28.);

							let close_button = ui.add(
								div()
									.text(icons::X)
									.size([28., 28.])
									.text_color(self.config.theme.red())
									.fill(Color32::TRANSPARENT)
									.cursor(CursorIcon::PointingHand)
									.hover_fill(self.config.theme.background_bright())
									.text_align(Align2::CENTER_CENTER)
									.corner_radius(4.),
							);

							if close_button.clicked() {
								self.screen = AppScreen::Soundboard;
							}
						});

						ui.separator();
						ui.add_space(8.);

						let available_height = ui.available_height();

						ui.horizontal_top(|ui| {
							ui.set_min_height(available_height);

							self.draw_settings_sidebar(ui);
							ui.separator();

							ui.vertical(|ui| match tab {
								SettingsTab::Appearance => self.draw_appearance_settings(ui),
								SettingsTab::Hotkeys => self.draw_hotkeys_settings(ui),
								SettingsTab::System => self.draw_system_settings(ui),
								SettingsTab::Sounds(id) => self.draw_sound_settings(ui, id),
								SettingsTab::Support => self.draw_support_settings(ui),
							});
						});
					});
			});
		});
	}

	fn draw_settings_sidebar(&mut self, ui: &mut Ui) {
		ui.allocate_ui_with_layout(vec2(170., ui.available_height()), Layout::top_down(Align::Min), |ui| {
			ui.vertical(|ui| {
				macro_rules! draw_sidebar_button {
					($name: expr, $icon: expr, $screen: expr) => {
						let button = ui.add(
							div()
								.text(format!("  {}   {}", $icon, $name))
								.font_size(14.)
								.text_align(Align2::LEFT_CENTER)
								.text_color(if self.screen == $screen {
									self.config.theme.background_dark()
								} else {
									self.config.theme.text()
								})
								.size([200., 28.])
								.corner_radius(4.)
								.stroke(Stroke::NONE)
								.cursor(CursorIcon::PointingHand)
								.hover_fill(if self.screen == $screen {
									self.config.theme.accent()
								} else {
									self.config.theme.background_bright()
								})
								.fill(if self.screen == $screen {
									self.config.theme.accent()
								} else {
									Color32::TRANSPARENT
								}),
						);

						if button.clicked() {
							self.screen = $screen;
						}
					};
				}

				draw_sidebar_button!("Appearance", icons::PALETTE, AppScreen::Settings(SettingsTab::Appearance));
				draw_sidebar_button!("Sounds", icons::SPEAKER_HIGH, AppScreen::Settings(SettingsTab::Sounds(None)));
				draw_sidebar_button!("System", icons::CPU, AppScreen::Settings(SettingsTab::System));
				draw_sidebar_button!("Keybindings", icons::KEYBOARD, AppScreen::Settings(SettingsTab::Hotkeys));
				draw_sidebar_button!("Support", icons::QUESTION, AppScreen::Settings(SettingsTab::Support));
			});
		});
	}

	fn draw_support_settings(&mut self, ui: &mut Ui) {
		empty_frame().inner_margin(8.).show(ui, |ui| {
			ui.vertical(|ui| {
				ui.horizontal(|ui| {
					ui.add(
						Label::new(
							RichText::new("zack i made the fucking app why are you looking in the support tab just text me if something isnt working you fucking idiot")
								.size(16.)
								.color(self.config.theme.text()),
						)
						.wrap(),
					);
				});
			});
		});
	}

	fn draw_sound_settings(&mut self, ui: &mut Ui, id: Option<Uuid>) {
		ui.visuals_mut().widgets.hovered = ui.visuals().widgets.inactive;
		ui.visuals_mut().widgets.active = ui.visuals().widgets.inactive;

		ui.horizontal_top(|ui| {
			ui.set_min_height(ui.available_height());
			ui.allocate_ui_with_layout(vec2(400., ui.available_height()), Layout::top_down(Align::Min), |ui| {
				ui.vertical(|ui| {
					ui.horizontal(|ui| {
						let mut page_name = self.config.soundboard.pages[self.settings_sound_page].name.clone();
						if page_name.is_empty() {
							page_name = format!("Page {}", self.settings_sound_page + 1);
						}
						ui.label(RichText::new(page_name.to_uppercase()).size(12.).color(self.config.theme.accent()));

						ui.add_space(ui.available_width() - 60.);

						if self.settings_sound_page > 0 {
							let page_left = ui.add(
								div()
									.text(icons::CARET_LEFT)
									.font_size(14.)
									.size([24., 24.])
									.text_color(self.config.theme.text())
									.hover_fill(self.config.theme.background_bright())
									.text_align(Align2::CENTER_CENTER)
									.corner_radius(4.)
									.cursor(CursorIcon::PointingHand),
							);
							if page_left.clicked() && self.settings_sound_page > 0 {
								self.settings_sound_page -= 1;
							}
						} else {
							ui.add(div().size([24., 24.]));
						}

						let page_right = ui.add(
							div()
								.text(if self.settings_sound_page < self.config.soundboard.pages.len() - 1 {
									icons::CARET_RIGHT
								} else {
									icons::PLUS
								})
								.font_size(14.)
								.size([24., 24.])
								.text_color(self.config.theme.text())
								.hover_fill(self.config.theme.background_bright())
								.text_align(Align2::CENTER_CENTER)
								.corner_radius(4.)
								.cursor(CursorIcon::PointingHand),
						);
						if page_right.hovered() {
							ui.set_cursor_icon(egui::CursorIcon::PointingHand);
						}
						if page_right.clicked() {
							if self.settings_sound_page == self.config.soundboard.pages.len() - 1 {
								self.config.soundboard.pages.push(SoundPage::new());
							};
							self.settings_sound_page += 1;
						}

						if self.config.soundboard.pages.len() > 1 {
							let delete_page = ui.add(
								Button::new(RichText::new(icons::TRASH).size(16.).color(self.config.theme.red()))
									.stroke(Stroke::NONE)
									.fill(self.config.theme.background()),
							);

							if delete_page.hovered() {
								ui.set_cursor_icon(egui::CursorIcon::PointingHand);
							}

							if delete_page.clicked() {
								self.popup = Some(Popup::DeletePage);
							}
						}
					});
				});

				let sb = &self.config.soundboard;
				let nodes = sb.pages[self.settings_sound_page].sounds.clone();

				for sound_node in nodes.iter().filter(|node| matches!(node, SoundTreeNode::Group(_))) {
					self.draw_sound_node(ui, sound_node.id());
				}

				for sound_node in nodes.iter().filter(|node| matches!(node, SoundTreeNode::Sound(_))) {
					self.draw_sound_node(ui, sound_node.id());
				}

				ui.add_space(ui.available_height() - 28.);

				ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
					button()
						.size([28., 28.])
						.text(icons::FILE_PLUS)
						.font_size(16.)
						.text_color(self.config.theme.text())
						.fill(self.config.theme.background_dim())
						.stroke(Stroke::new(1., self.config.theme.background_bright()))
						.hover_fill(self.config.theme.background_bright())
						.add_to(ui)
						.on_click(|| {
							ui.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
							self.picking_file = Some(None);
						});

					ui.add_space(4.);

					button()
						.size([28., 28.])
						.text(icons::FOLDER_SIMPLE_PLUS)
						.font_size(16.)
						.text_color(self.config.theme.text())
						.fill(self.config.theme.background_dim())
						.stroke(Stroke::new(1., self.config.theme.background_bright()))
						.hover_fill(self.config.theme.background_bright())
						.add_to(ui)
						.on_click(|| {
							let id = Uuid::new_v4();
							self.config.soundboard.pages[self.settings_sound_page].sounds.push(SoundTreeNode::Group(SoundGroup {
								name: "New Group".to_owned(),
								icon: Some(egui_phosphor::regular::PACKAGE.to_owned()),
								id,
								sounds: Vec::new(),
							}));
							self.open_groups.insert(id);
							self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(id)));
						});

					ui.add_space(4.);
				});
			});

			ui.separator();

			if let Some(id) = id {
				let sound = self.config.soundboard.get_sound_mut(id);
				if let Some(sound) = sound {
					empty_frame().inner_margin(8.).show(ui, |ui| {
						ui.vertical(|ui| {
							ui.horizontal(|ui| {
								ui.set_max_height(28.);
								ui.set_min_height(28.);
								ui.label(RichText::new("Name:").size(16.).color(self.config.theme.text()));

								ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
								ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
								ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(1., self.config.theme.background_bright());

								ui.visuals_mut().widgets.inactive.weak_bg_fill = self.config.theme.background_dim();
								ui.visuals_mut().widgets.hovered.weak_bg_fill = self.config.theme.background_dim();
								ui.visuals_mut().widgets.active.weak_bg_fill = self.config.theme.background_dim();

								ui.visuals_mut().widgets.inactive.bg_fill = self.config.theme.background_dim();
								ui.visuals_mut().widgets.hovered.bg_fill = self.config.theme.background_dim();
								ui.visuals_mut().widgets.active.bg_fill = self.config.theme.background_dim();

								ui.visuals_mut().extreme_bg_color = self.config.theme.background_dim();

								ui.visuals_mut().widgets.inactive.fg_stroke = Stroke::new(1., self.config.theme.text());
								ui.visuals_mut().widgets.hovered.fg_stroke = Stroke::new(1., self.config.theme.text());
								ui.visuals_mut().widgets.active.fg_stroke = Stroke::new(1., self.config.theme.text());

								ui.spacing_mut().interact_size.y = 28.;

								ui.add_sized(vec2(200., 28.), TextEdit::singleline(&mut sound.name).vertical_align(Align::Center));
							});

							ui.add_space(4.);

							ui.horizontal(|ui| {
								ui.set_max_height(28.);
								ui.set_min_height(28.);
								ui.label(RichText::new("Icon:").size(16.).color(self.config.theme.text()));

								ui.add_space(4.);

								button()
									.size([28., 28.])
									.text(sound.icon.as_ref().cloned().unwrap_or_default())
									.fill(self.config.theme.background_dim())
									.hover_fill(self.config.theme.background_bright())
									.text_color(self.config.theme.text())
									.stroke(Stroke::new(1., self.config.theme.background_bright()))
									.add_to(ui)
									.on_click(|| self.icon_picker_coords = ui.pointer_latest_pos());
							});

							ui.add_space(4.);

							App::draw_volume_slider(ui, "Volume for me:", &mut sound.volume_for_me, 0..=100, self.config.theme);

							ui.add_space(4.);

							App::draw_volume_slider(ui, "Volume for others:", &mut sound.volume_for_others, 0..=100, self.config.theme);

							ui.add_space(ui.available_height() - 24.);

							ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
								button()
									.size([28., 28.])
									.text(icons::TRASH)
									.font_size(16.)
									.text_color(self.config.theme.red())
									.fill(self.config.theme.background_dim())
									.stroke(Stroke::new(1., self.config.theme.background_bright()))
									.hover_fill(self.config.theme.background_bright())
									.add_to(ui)
									.on_click(|| {
										self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(sound.id)));
										self.popup = Some(Popup::DeleteSound);
										ui.request_repaint();
									});
							})
						});
					});
				} else if let Some(SoundTreeNode::Group(group)) = self.config.soundboard.get_sound_node_mut(id) {
					empty_frame().inner_margin(8.).show(ui, |ui| {
						ui.vertical(|ui| {
							ui.horizontal(|ui| {
								ui.set_max_height(28.);
								ui.set_min_height(28.);
								ui.label(RichText::new("Name:").size(16.).color(self.config.theme.text()));

								ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
								ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
								ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(1., self.config.theme.background_bright());

								ui.visuals_mut().widgets.inactive.weak_bg_fill = self.config.theme.background_dim();
								ui.visuals_mut().widgets.hovered.weak_bg_fill = self.config.theme.background_dim();
								ui.visuals_mut().widgets.active.weak_bg_fill = self.config.theme.background_dim();

								ui.visuals_mut().widgets.inactive.bg_fill = self.config.theme.background_dim();
								ui.visuals_mut().widgets.hovered.bg_fill = self.config.theme.background_dim();
								ui.visuals_mut().widgets.active.bg_fill = self.config.theme.background_dim();

								ui.visuals_mut().extreme_bg_color = self.config.theme.background_dim();

								ui.visuals_mut().widgets.inactive.fg_stroke = Stroke::new(1., self.config.theme.text());
								ui.visuals_mut().widgets.hovered.fg_stroke = Stroke::new(1., self.config.theme.text());
								ui.visuals_mut().widgets.active.fg_stroke = Stroke::new(1., self.config.theme.text());

								ui.spacing_mut().interact_size.y = 28.;

								ui.add_sized(vec2(200., 28.), TextEdit::singleline(&mut group.name).vertical_align(Align::Center));
							});

							ui.add_space(4.);

							ui.horizontal(|ui| {
								ui.set_max_height(28.);
								ui.set_min_height(28.);
								ui.label(RichText::new("Icon:").size(16.).color(self.config.theme.text()));

								ui.add_space(4.);

								button()
									.size([28., 28.])
									.text(group.icon.as_ref().cloned().unwrap_or_default())
									.fill(self.config.theme.background_dim())
									.hover_fill(self.config.theme.background_bright())
									.text_color(self.config.theme.text())
									.stroke(Stroke::new(1., self.config.theme.background_bright()))
									.add_to(ui)
									.on_click(|| self.icon_picker_coords = ui.pointer_latest_pos());
							});

							ui.add_space(ui.available_height() - 24.);

							ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
								button()
									.size([28., 28.])
									.text(icons::TRASH)
									.font_size(16.)
									.text_color(self.config.theme.red())
									.fill(self.config.theme.background_dim())
									.stroke(Stroke::new(1., self.config.theme.background_bright()))
									.hover_fill(self.config.theme.background_bright())
									.add_to(ui)
									.on_click(|| {
										self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(group.id)));
										self.popup = Some(Popup::DeleteGroup);
										ui.request_repaint();
									});

								ui.add_space(4.);

								button()
									.size([28., 28.])
									.text(icons::FILE_PLUS)
									.font_size(16.)
									.text_color(self.config.theme.text())
									.fill(self.config.theme.background_dim())
									.stroke(Stroke::new(1., self.config.theme.background_bright()))
									.hover_fill(self.config.theme.background_bright())
									.add_to(ui)
									.on_click(|| {
										ui.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
										self.picking_file = Some(Some(group.id));
									});

								ui.add_space(4.);

								button()
									.size([28., 28.])
									.text(icons::FOLDER_SIMPLE_PLUS)
									.font_size(16.)
									.text_color(self.config.theme.text())
									.fill(self.config.theme.background_dim())
									.stroke(Stroke::new(1., self.config.theme.background_bright()))
									.hover_fill(self.config.theme.background_bright())
									.add_to(ui)
									.on_click(|| {
										let id = Uuid::new_v4();
										group.sounds.push(SoundTreeNode::Group(SoundGroup {
											name: "New Group".to_owned(),
											icon: Some(egui_phosphor::regular::PACKAGE.to_owned()),
											sounds: Vec::new(),
											id,
										}));
										self.open_groups.insert(group.id);
										self.open_groups.insert(id);
										self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(id)));
										ui.request_repaint();
									});

								ui.add_space(4.);
							});
						});
					});
				}
			}
		});
	}

	fn pick_file(&mut self, ui: &mut Ui) {
		if let Some(id) = self.picking_file
			&& let Some(file) = rfd::FileDialog::new().add_filter("Audio files", &["mp3"]).pick_file()
			&& let Ok(bytes) = std::fs::read(&file)
		{
			let data = SoundData::new(bytes);
			let file_name = file.file_name().and_then(|name| name.to_str());

			if let Ok(data) = data
				&& let Some(file_name) = file_name
			{
				if let Some(SoundTreeNode::Group(group)) = id.and_then(|id| self.config.soundboard.get_sound_node_mut(id)) {
					group.sounds.push(SoundTreeNode::Sound(Sound {
						name: file_name.to_owned(),
						data,
						icon: Some(egui_phosphor::regular::PAW_PRINT.to_owned()),
						id: Uuid::new_v4(),
						volume_for_me: 100,
						volume_for_others: 100,
					}))
				} else {
					self.config.soundboard.pages[self.settings_sound_page].sounds.push(SoundTreeNode::Sound(Sound {
						name: file_name.to_owned(),
						data,
						icon: Some(egui_phosphor::regular::PAW_PRINT.to_owned()),
						id: Uuid::new_v4(),
						volume_for_me: 100,
						volume_for_others: 100,
					}))
				}
			}
		}

		self.picking_file = None;
		ui.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
	}

	fn draw_icon_picker(&mut self, ui: &mut Ui) {
		let columns = 10;
		let rows = 5;
		let gap = 8.;
		let button_size = 28.;
		if let Some(pointer) = self.icon_picker_coords {
			let icon_picker = Area::new("icon-picker".into()).fixed_pos(pointer).default_size(vec2(10., 10.)).show(ui, |ui| {
				div()
					.size([(button_size + gap) * columns as f32 + gap, (button_size + gap) * rows as f32 + button_size + gap * 1.5])
					.fill(self.config.theme.background_dark())
					.stroke(Stroke::new(1., self.config.theme.background_bright()))
					.corner_radius(8.)
					.inner_margin(8.)
					.shadow(Shadow {
						offset: [0, 0],
						blur: 8,
						spread: 2,
						color: Color32::BLACK,
					})
					.show(ui, |ui| {
						ui.vertical(|ui| {
							let icons = icons::ICONS
								.iter()
								.filter(|(name, _icon)| name.replace(" ", "").to_lowercase().contains(&self.icon_picker_search_text.to_lowercase().replace("_", "")))
								.collect::<Vec<_>>();

							ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
							ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
							ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(1., self.config.theme.background_bright());

							ui.visuals_mut().widgets.inactive.weak_bg_fill = self.config.theme.background_dim();
							ui.visuals_mut().widgets.hovered.weak_bg_fill = self.config.theme.background_dim();
							ui.visuals_mut().widgets.active.weak_bg_fill = self.config.theme.background_dim();

							ui.visuals_mut().widgets.inactive.bg_fill = self.config.theme.background_dim();
							ui.visuals_mut().widgets.hovered.bg_fill = self.config.theme.background_dim();
							ui.visuals_mut().widgets.active.bg_fill = self.config.theme.background_dim();

							ui.visuals_mut().extreme_bg_color = self.config.theme.background_dim();

							ui.visuals_mut().widgets.inactive.fg_stroke = Stroke::new(1., self.config.theme.text());
							ui.visuals_mut().widgets.hovered.fg_stroke = Stroke::new(1., self.config.theme.text());
							ui.visuals_mut().widgets.active.fg_stroke = Stroke::new(1., self.config.theme.text());

							ui.spacing_mut().interact_size.y = 28.;

							ui.add_sized(
								vec2((28. + 7.) * columns as f32, 28.),
								TextEdit::singleline(&mut self.icon_picker_search_text).vertical_align(Align::Center).hint_text("Search..."),
							);

							ui.add_space(4.);

							ui.vertical(|ui| {
								for row in 0..rows {
									if row > rows {
										break;
									}

									ui.horizontal(|ui| {
										for column in 0..columns {
											let index = row * columns + column;
											if index < icons.len() {
												let icon = icons[index].1;
												button()
													.text(icon)
													.size([28., 28.])
													.text_color(self.config.theme.text())
													.fill(self.config.theme.background())
													.hover_fill(self.config.theme.background_bright())
													.stroke(Stroke::new(1., self.config.theme.background_bright()))
													.add_to(ui)
													.on_click(|| {
														if let AppScreen::Settings(SettingsTab::Sounds(Some(sound_id))) = &self.screen
															&& let Some(sound) = self.config.soundboard.get_sound_node_mut(*sound_id)
														{
															sound.set_icon(icon);
															self.icon_picker_coords = None;
														}
													});
											}
										}
									});

									if row < rows {
										ui.add_space(4.);
									}
								}
							});
						});
					});
			});

			if icon_picker.response.clicked_elsewhere() {
				self.icon_picker_coords = None;
			}
		}
	}

	fn draw_hotkeys_settings(&mut self, ui: &mut Ui) {
		empty_frame().inner_margin(8.).show(ui, |ui| {
			ui.vertical(|ui| {
				ui.label(
					RichText::new(format!(" {}   Global keybindings are activated even if the soundboard overlay isn't open.", icons::INFO))
						.size(16.)
						.color(self.config.theme.text()),
				);
				ui.add_space(20.);
				ui.horizontal(|ui| {
					ui.allocate_ui_with_layout(vec2(200., ui.available_height()), Layout::top_down(Align::Min), |ui| {
						ui.label(RichText::new(format!(" {}   Key", icons::KEYBOARD)).size(16.).color(self.config.theme.text()));
						ui.separator();
						ui.add_space(4.);
						for keybinding in self.config.keybindings.lock().unwrap().iter() {
							button()
								.size([ui.available_width(), 28.])
								.text(if self.recording_keybinding.is_some_and(|id| id == keybinding.id) {
									"Recording...".to_owned()
								} else {
									format!("{}", keybinding.key)
								})
								.font_size(14.)
								.text_color(self.config.theme.text())
								.fill(self.config.theme.background_dim())
								.hover_fill(self.config.theme.background_bright())
								.stroke(Stroke::new(1., self.config.theme.background_bright()))
								.add_to(ui)
								.on_click(|| {
									self.recording_keybinding = Some(keybinding.id);
									self.is_recording.store(true, Ordering::Relaxed);
								});
						}
					});

					ui.separator();

					ui.allocate_ui_with_layout(vec2(175., ui.available_height()), Layout::top_down(Align::Min), |ui| {
						ui.label(RichText::new(format!(" {}   Action", icons::LIGHTNING)).size(16.).color(self.config.theme.text()));
						ui.separator();
						ui.add_space(4.);

						for keybinding in self.config.keybindings.lock().unwrap().iter_mut() {
							ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
							ui.visuals_mut().widgets.active.weak_bg_fill = self.config.theme.background_bright();
							ui.visuals_mut().widgets.inactive.weak_bg_fill = self.config.theme.background_dim();
							ui.visuals_mut().widgets.hovered.weak_bg_fill = self.config.theme.background_bright();
							ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
							ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
							ui.visuals_mut().widgets.open.weak_bg_fill = self.config.theme.background_bright();
							ui.visuals_mut().widgets.open.bg_stroke = Stroke::new(1., self.config.theme.background_bright());

							ui.spacing_mut().interact_size.y = 28.;
							let action_selector = ComboBox::from_id_salt(format!("hotkey-action-{}", keybinding.id))
								.selected_text(RichText::new(keybinding.action.name()).size(14.).color(self.config.theme.text()))
								.popup_style(StyleModifier::new(|style| style.visuals.window_fill = self.config.theme.background_dim()))
								.width(ui.available_width())
								.show_ui(ui, |ui| {
									for action in UiAction::iter() {
										button()
											.size([175., 20.])
											.text(format!("  {}", action.name()))
											.text_align(Align2::LEFT_CENTER)
											.fill(self.config.theme.background_dim())
											.text_color(self.config.theme.text())
											.hover_fill(self.config.theme.background_bright())
											.font_size(14.)
											.add_to(ui)
											.on_click(|| keybinding.action = action);
									}
								});

							if action_selector.response.hovered() {
								ui.set_cursor_icon(egui::CursorIcon::PointingHand);
							}
						}
					});

					ui.separator();

					ui.allocate_ui_with_layout(vec2(100., ui.available_height()), Layout::top_down(Align::Min), |ui| {
						ui.label(RichText::new(format!(" {}   Behavior", icons::HAND_POINTING)).size(16.).color(self.config.theme.text()));
						ui.separator();
						ui.add_space(4.);
						for keybinding in self.config.keybindings.lock().unwrap().iter_mut() {
							ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
							ui.visuals_mut().widgets.active.weak_bg_fill = self.config.theme.background_bright();
							ui.visuals_mut().widgets.inactive.weak_bg_fill = self.config.theme.background_dim();
							ui.visuals_mut().widgets.hovered.weak_bg_fill = self.config.theme.background_bright();
							ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
							ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
							ui.visuals_mut().widgets.open.weak_bg_fill = self.config.theme.background_bright();
							ui.visuals_mut().widgets.open.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
							ui.spacing_mut().interact_size.y = 28.;
							let behavior_selector = ComboBox::from_id_salt(format!("hotkey-behavior-{}", keybinding.id))
								.selected_text(RichText::new(keybinding.behavior.name()).size(14.).color(self.config.theme.text()))
								.popup_style(StyleModifier::new(|style| style.visuals.window_fill = self.config.theme.background_dim()))
								.width(ui.available_width())
								.show_ui(ui, |ui| {
									for behavior in HotkeyBehavior::iter() {
										button()
											.text(format!("  {}", behavior.name()))
											.size([100., 20.])
											.font_size(14.)
											.hover_fill(self.config.theme.background_bright())
											.text_color(self.config.theme.text())
											.text_align(Align2::LEFT_CENTER)
											.fill(self.config.theme.background_dim())
											.add_to(ui)
											.on_click(|| keybinding.behavior = behavior);
									}
								});

							if behavior_selector.response.hovered() {
								ui.set_cursor_icon(egui::CursorIcon::PointingHand);
							}
						}
					});

					ui.separator();

					ui.allocate_ui_with_layout(vec2(150., ui.available_height()), Layout::top_down(Align::Min), |ui| {
						ui.label(RichText::new(format!(" {}   Value", icons::PUZZLE_PIECE)).size(16.).color(self.config.theme.text()));
						ui.separator();
						ui.add_space(4.);
						for keybinding in self.config.keybindings.lock().unwrap().iter_mut() {
							ui.visuals_mut().widgets.inactive.weak_bg_fill = self.config.theme.background_dim();
							ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., Color32::from_hex("#313244").unwrap());
							ui.spacing_mut().interact_size.y = 28.;

							if let Some(argument) = keybinding.action.arguments().first() {
								let sound = keybinding.arguments.get(argument.name).and_then(|inner| match inner {
									KeybindingArgumentValue::Uuid(uuid) => self.config.soundboard.get_sound_node(*uuid),
								});

								ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
								ui.visuals_mut().widgets.active.weak_bg_fill = self.config.theme.background_bright();
								ui.visuals_mut().widgets.inactive.weak_bg_fill = self.config.theme.background_dim();
								ui.visuals_mut().widgets.hovered.weak_bg_fill = self.config.theme.background_bright();
								ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
								ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
								ui.visuals_mut().widgets.open.weak_bg_fill = self.config.theme.background_bright();
								ui.visuals_mut().widgets.open.bg_stroke = Stroke::new(1., self.config.theme.background_bright());

								ui.spacing_mut().interact_size.y = 28.;
								let argument_selector = ComboBox::from_id_salt(format!("hotkey-arguments-{}", keybinding.id))
									.selected_text(
										RichText::new(sound.map(|sound| format!(" {}   {}", sound.icon().map_or("", |v| v), sound.name())).unwrap_or_default())
											.size(14.)
											.color(self.config.theme.text()),
									)
									.popup_style(StyleModifier::new(|style| style.visuals.window_fill = self.config.theme.background_dim()))
									.width(ui.available_width())
									.show_ui(ui, |ui| match argument.argument_type {
										ArgumentType::Sound => {
											for sound in self.config.soundboard.sounds() {
												button()
													.size([150., 20.])
													.text(format!(" {}   {}", sound.icon.as_ref().map_or("", |v| v), sound.name))
													.text_align(Align2::LEFT_CENTER)
													.fill(self.config.theme.background_dim())
													.text_color(self.config.theme.text())
													.hover_fill(self.config.theme.background_bright())
													.font_size(14.)
													.add_to(ui)
													.on_click(|| {
														keybinding.arguments.insert(argument.name.to_owned(), KeybindingArgumentValue::Uuid(sound.id));
													});
											}
										},
									});
								if argument_selector.response.hovered() {
									ui.set_cursor_icon(egui::CursorIcon::PointingHand);
								}
							} else {
								ui.allocate_ui(vec2(ui.available_width(), 28.), |ui| {
									ui.take_available_space();
								});
							}
						}
					});

					ui.separator();

					ui.allocate_ui_with_layout(vec2(80., ui.available_height()), Layout::top_down(Align::Min), |ui| {
						ui.label(
							RichText::new(format!(" {}   Global", icons::GLOBE_HEMISPHERE_WEST))
								.size(16.)
								.color(self.config.theme.text()),
						);
						ui.separator();
						ui.add_space(4.);
						ui.vertical_centered(|ui| {
							for keybinding in self.config.keybindings.lock().unwrap().iter_mut() {
								if keybinding.action.force_global() {
									keybinding.global = true;
								} else if keybinding.action.force_local() {
									keybinding.global = false;
								}
								let mut is_global = keybinding.global;

								button()
									.size([28., 28.])
									.fill(self.config.theme.background_dim())
									.stroke(Stroke::new(1., self.config.theme.background_bright()))
									.text(if is_global { icons::CHECK } else { icons::X })
									.text_color(if is_global { self.config.theme.green() } else { self.config.theme.red() })
									.cursor(if keybinding.action.force_global() {
										CursorIcon::NotAllowed
									} else {
										CursorIcon::PointingHand
									})
									.hover_fill(if keybinding.action.force_global() {
										self.config.theme.background_dark()
									} else {
										self.config.theme.background_bright()
									})
									.add_to(ui)
									.on_click(|| {
										if keybinding.action.force_global() {
											return;
										}

										is_global = !is_global;
										keybinding.global = is_global;

										if let Some(hotkey) = keybinding.key.to_global() {
											if is_global {
												let _ = self.hotkey_manager.register(hotkey);
											} else {
												let _ = self.hotkey_manager.unregister(hotkey);
											}
										}

										ui.request_repaint();
									});
							}
						});
					});

					ui.separator();

					ui.allocate_ui_with_layout(vec2(50., ui.available_height()), Layout::top_down(Align::Min), |ui| {
						ui.label(RichText::new("").size(16.).color(self.config.theme.text()));
						ui.separator();
						ui.add_space(4.);
						ui.vertical_centered(|ui| {
							let mut delete = None;
							for keybinding in self.config.keybindings.lock().unwrap().iter_mut() {
								button()
									.size([28., 28.])
									.text(icons::TRASH)
									.font_size(16.)
									.text_color(self.config.theme.red())
									.fill(self.config.theme.background_dim())
									.hover_fill(self.config.theme.background_bright())
									.stroke(Stroke::new(1., self.config.theme.background_bright()))
									.add_to(ui)
									.on_click(|| delete = Some(keybinding.clone()))
							}

							if let Some(keybinding) = delete {
								if keybinding.global {
									keybinding.key.to_global().and_then(|key| self.hotkey_manager.unregister(key).ok());
								}
								self.config.keybindings.lock().unwrap().retain(|kb| kb != &keybinding);
							}

							ui.add_space(8.);

							button()
								.size([28., 28.])
								.text(icons::PLUS)
								.font_size(16.)
								.text_color(self.config.theme.text())
								.stroke(Stroke::new(1., self.config.theme.background_bright()))
								.fill(self.config.theme.background_dim())
								.hover_fill(self.config.theme.background_bright())
								.add_to(ui)
								.on_click(|| {
									self.config.keybindings.lock().unwrap().push(Keybinding {
										key: LocalKeybinding {
											key: Key::Space,
											modifiers: Modifiers::CTRL,
										},
										behavior: HotkeyBehavior::Press,
										action: UiAction::ToggleSoundboard,
										global: false,
										id: Uuid::new_v4(),
										arguments: HashMap::new(),
									});
								});
						});
					});
				});
			});
		});
	}

	fn draw_appearance_settings(&mut self, ui: &mut Ui) {
		empty_frame().inner_margin(8.).show(ui, |ui| {
			ui.columns(2, |ui| {
				ui[0].vertical(|ui| {
					ui.horizontal(|ui| {
						ui.set_max_height(28.);
						ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.active.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.inactive.weak_bg_fill = self.config.theme.background_dim();
						ui.visuals_mut().widgets.hovered.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.open.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.open.bg_stroke = Stroke::new(1., self.config.theme.background_bright());

						ui.spacing_mut().interact_size.y = 28.;
						ui.label(RichText::new("Theme: ").color(self.config.theme.text()).size(16.));
						let theme_selector = ComboBox::from_id_salt("themes")
							.selected_text(
								RichText::new(format!(
									"{}{}",
									self.config.theme.icon().as_ref().map(|icon| format!("{icon}   ")).unwrap_or_default(),
									self.config.theme.name()
								))
								.size(16.)
								.color(self.config.theme.text()),
							)
							.popup_style(StyleModifier::new(|style| style.visuals.window_fill = self.config.theme.background_dim()))
							.width(200.)
							.show_ui(ui, |ui| {
								for theme in THEMES {
									button()
										.size([200., 20.])
										.text(format!("  {}{}", theme.icon().as_ref().map(|icon| format!("{icon}   ")).unwrap_or_default(), theme.name()))
										.text_align(Align2::LEFT_CENTER)
										.fill(self.config.theme.background_dim())
										.text_color(self.config.theme.text())
										.hover_fill(self.config.theme.background_bright())
										.font_size(14.)
										.add_to(ui)
										.on_click(|| self.config.theme = theme);
								}
							});

						if theme_selector.response.hovered() {
							ui.set_cursor_icon(CursorIcon::PointingHand);
						}
					});

					ui.add_space(4.);

					App::draw_volume_slider(ui, "Background opacity:", &mut self.config.background_opacity, 0. ..=255., self.config.theme);
				});
			});
		});
	}

	fn draw_system_settings(&mut self, ui: &mut Ui) {
		empty_frame().inner_margin(8.).show(ui, |ui| {
			ui.vertical(|ui| {
				ui.horizontal(|ui| {
					ui.set_max_height(28.);
					ui.label(RichText::new(format!(" {}   Output device:", icons::HEADSET)).color(self.config.theme.text()).size(14.));

						ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.active.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.inactive.weak_bg_fill = self.config.theme.background_dim();
						ui.visuals_mut().widgets.hovered.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.open.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.open.bg_stroke = Stroke::new(1., self.config.theme.background_bright());

						ui.spacing_mut().interact_size.y = 28.;

					let output_device_selector = egui::ComboBox::from_id_salt("output-devices")
						.selected_text(RichText::new(format!("   {}   ", self.config.output_device.as_ref().and_then(|device| device.description().map(|desc| desc.extended().join(", ")).ok()).unwrap_or_default())).color(self.config.theme.text()))
						.width(350.)
							.popup_style(StyleModifier::new(|style| style.visuals.window_fill = self.config.theme.background_dim()))
						.show_ui(ui, |ui| {
							for device in AudioManager::output_devices() {
								button()
									.size([350., 20.])
									.text(format!("  {}", device.description().unwrap().extended().join(", ")))
									.text_align(Align2::LEFT_CENTER)
									.fill(self.config.theme.background_dim())
									.text_color(self.config.theme.text())
									.hover_fill(self.config.theme.background_bright())
									.font_size(14.)
									.add_to(ui)
									.on_click(|| {
										self.config.output_device = Some(device.clone());
										self.audio_player.lock().unwrap().set_devices(device, self.config.input_device.as_ref().unwrap().clone(), self.config.virtual_cable_device.as_ref().unwrap().clone());
									});
							}
						});
					if output_device_selector.response.hovered() {
						ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
					}
				});


				ui.add_space(4.);
				ui.label(
					RichText::new(
						"Set this to your headphones or speaker."
					)
					.color(self.config.theme.text_dim()),
				);
				ui.add_space(20.);
				ui.horizontal(|ui| {
					ui.set_max_height(28.);
					ui.label(RichText::new(format!(" {}   Input device:", icons::MICROPHONE)).color(self.config.theme.text()).size(14.));
						ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.active.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.inactive.weak_bg_fill = self.config.theme.background_dim();
						ui.visuals_mut().widgets.hovered.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.open.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.open.bg_stroke = Stroke::new(1., self.config.theme.background_bright());

						ui.spacing_mut().interact_size.y = 28.;
					let input_device_selector = egui::ComboBox::new("input-devices", "")
					.width(350.)
					.selected_text(RichText::new(format!("   {}   ", self.config.input_device.as_ref().and_then(|device| device.description().map(|desc| desc.extended().join(", ")).ok()).unwrap_or_default())).color(self.config.theme.text()))
							.popup_style(StyleModifier::new(|style| style.visuals.window_fill = self.config.theme.background_dim()))
					.show_ui(ui, |ui| {
						for device in AudioManager::input_devices() {
								button()
									.size([350., 20.])
									.text(format!("  {}", device.description().unwrap().extended().join(", ")))
									.text_align(Align2::LEFT_CENTER)
									.fill(self.config.theme.background_dim())
									.text_color(self.config.theme.text())
									.hover_fill(self.config.theme.background_bright())
									.font_size(14.)
									.add_to(ui)
									.on_click(|| {
										self.config.input_device = Some(device.clone());
										self.audio_player.lock().unwrap().set_devices(self.config.output_device.as_ref().unwrap().clone(), device, self.config.virtual_cable_device.as_ref().unwrap().clone());
									});
						}
					});
					if input_device_selector.response.hovered() {
						ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
					}
				});

				ui.add_space(4.);

				ui.label(
					RichText::new(
						"Set this to your microphone device."
					)
					.color(self.config.theme.text_dim()),
				);

				ui.add_space(20.);

				ui.horizontal(|ui| {
					ui.set_max_height(28.);
					ui.label(RichText::new(format!(" {}   Virtual cable device:", icons::PLUGS)).color(self.config.theme.text()).size(14.));
						ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.active.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.inactive.weak_bg_fill = self.config.theme.background_dim();
						ui.visuals_mut().widgets.hovered.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(1., self.config.theme.background_bright());
						ui.visuals_mut().widgets.open.weak_bg_fill = self.config.theme.background_bright();
						ui.visuals_mut().widgets.open.bg_stroke = Stroke::new(1., self.config.theme.background_bright());

						ui.spacing_mut().interact_size.y = 28.;
					let cable_device_selector = egui::ComboBox::new("vb-cable-device", "")
					.width(350.)
							.popup_style(StyleModifier::new(|style| style.visuals.window_fill = self.config.theme.background_dim()))
					.selected_text(RichText::new(format!("   {}   ", self.config.virtual_cable_device.as_ref().and_then(|device| device.description().map(|desc| desc.extended().join(", ")).ok()).unwrap_or_default())).color(self.config.theme.text())).show_ui(ui, |ui| {
						for device in AudioManager::output_devices() {
							button()
								.size([350., 20.])
								.text(format!("  {}", device.description().unwrap().extended().join(", ")))
								.text_align(Align2::LEFT_CENTER)
								.fill(self.config.theme.background_dim())
								.text_color(self.config.theme.text())
								.hover_fill(self.config.theme.background_bright())
								.font_size(14.)
								.add_to(ui)
								.on_click(|| {
									self.config.input_device = Some(device.clone());
									self.audio_player.lock().unwrap().set_devices(self.config.output_device.as_ref().unwrap().clone(), self.config.input_device.as_ref().unwrap().clone(), device);
								});
						}
					});
					if cable_device_selector.response.hovered() {
						ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
					}
				});

				ui.add_space(4.);

				ui.label(
					RichText::new(
						"This is a virtual device that comes installed with Zackophany that allows others to hear your soundboard. It should be called something like \"CABLE Input\". Set this as the input device in your chat app (discord, teams, etc.) for others to be able to hear your sound effects.",
					)
					.color(self.config.theme.text_dim()),
				);

				ui.add_space(16.);

				App::draw_volume_slider(ui, "UI Volume:", &mut self.config.ui_volume, 0. ..= 5., self.config.theme);
			});
		});
	}

	fn draw_sound_node(&mut self, ui: &mut Ui, id: Uuid) {
		let sb = &self.config.soundboard;
		let sound_node = sb.get_sound_node(id).unwrap().clone();

		ui.vertical(|ui| {
			let mut label = if let Some(icon) = sound_node.icon() {
				format!("   {}   {}", icon, sound_node.name())
			} else {
				format!("    {}", sound_node.name())
			};

			if sound_node.is_group() {
				let icon = if self.open_groups.contains(&id) { icons::CARET_DOWN } else { icons::CARET_RIGHT };
				label = format!("{label}      {icon}");
			}

			let sound_button = ui.add(
				button()
					.size([200., 22.])
					.text(label)
					.font_size(14.)
					.text_align(Align2::LEFT_CENTER)
					.hover_fill(self.config.theme.background_bright())
					.stroke(Stroke::NONE)
					.fill(
						if let AppScreen::Settings(SettingsTab::Sounds(Some(id))) = &self.screen
							&& id == &sound_node.id()
						{
							self.config.theme.background_bright()
						} else {
							Color32::TRANSPARENT
						},
					)
					.text_color(
						if let AppScreen::Settings(SettingsTab::Sounds(Some(id))) = &self.screen
							&& id == &sound_node.id()
						{
							self.config.theme.text_bright()
						} else {
							self.config.theme.text()
						},
					),
			);

			if sound_button.clicked() {
				if sound_node.is_group() {
					if self.open_groups.contains(&id) {
						self.open_groups.remove(&id);
					} else {
						self.open_groups.insert(id);
					}
				}
				self.screen = AppScreen::Settings(SettingsTab::Sounds(Some(id)));
			}

			if let SoundTreeNode::Group(group) = sound_node
				&& self.open_groups.contains(&id)
			{
				empty_frame()
					.inner_margin(Margin {
						left: 24,
						right: 0,
						top: 0,
						bottom: 0,
					})
					.show(ui, |ui| {
						ui.vertical(|ui| {
							for child_node in group.sounds.iter().filter(|node| SoundTreeNode::is_group(node)) {
								self.draw_sound_node(ui, child_node.id());
							}

							for child_node in group.sounds.iter().filter(|node| SoundTreeNode::is_sound(node)) {
								self.draw_sound_node(ui, child_node.id());
							}
						});
					});
			}
		});
	}

	fn draw_page_menu(&mut self, ui: &mut Ui) {
		Area::new("page_name".into()).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -200.0)).show(ui, |ui| {
			ui.horizontal(|ui| {
				let mut page_name = self.config.soundboard.pages[self.page].name.clone();
				if page_name.is_empty() {
					page_name = format!("Page {}", self.page + 1);
				}
				ui.label(RichText::new(page_name).color(self.config.theme.accent()));
			});
		});

		Area::new("page_selector".into())
			.anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 200.0))
			.show(ui, |ui| {
				ui.horizontal(|ui| {
					ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::NONE;
					ui.visuals_mut().widgets.hovered = ui.visuals().widgets.inactive;
					ui.visuals_mut().widgets.active = ui.visuals().widgets.inactive;
					let page_left = ui.add(
						Button::new(RichText::new(icons::CARET_LEFT).color(self.config.theme.accent()).size(16.))
							.fill(Color32::from_hex("#11111bcc").unwrap())
							.min_size(Vec2::splat(30.))
							.corner_radius(15.),
					);

					if ui.rect_contains_pointer(page_left.rect) {
						ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
					}

					if page_left.clicked() && self.page > 0 {
						self.page -= 1;
					}

					ui.add_space(100.);

					ui.label(
						RichText::new(format!("{} / {}", self.page + 1, self.config.soundboard.pages.len()))
							.color(self.config.theme.accent())
							.size(16.),
					);

					ui.add_space(100.);

					button()
						.text(icons::CARET_RIGHT)
						.text_color(self.config.theme.accent())
						.font_size(16.)
						.size([30., 30.])
						.corner_radius(15.)
						.fill(self.config.theme.sound_background())
						.add_to(ui)
						.on_click(|| {
							if self.page < self.config.soundboard.pages.len() - 1 {
								self.page += 1;
							}
						});
				});
			});
	}

	fn draw_soundboard(&mut self, ui: &mut Ui, overlay_frame: eframe::egui::Frame) {
		egui::CentralPanel::default().frame(overlay_frame).show(ui, |ui| {
			self.draw_page_menu(ui);
			self.draw_menu(ui);
			self.draw_config_buttons(ui);
			self.draw_debug_info(ui);
		});
	}

	pub fn listen_for_keybinding(&mut self, ui: &mut Ui) {
		if self.is_recording.load(Ordering::Relaxed) || ui.ctx().egui_wants_keyboard_input() {
			return;
		}

		let mut fired_keys = Vec::new();

		ui.input(|input| {
			for event in &input.events {
				if let Event::Key {
					key, pressed, repeat, modifiers, ..
				} = event
				{
					if *repeat {
						continue;
					}

					if matches!(
						key,
						Key::ControlLeft | Key::ShiftLeft | Key::ControlRight | Key::AltLeft | Key::AltRight | Key::SuperLeft | Key::SuperRight
					) {
						continue;
					}

					let behavior = if *pressed { HotkeyBehavior::Press } else { HotkeyBehavior::Release };

					fired_keys.push((LocalKeybinding { key: *key, modifiers: *modifiers }, behavior));
				}
			}
		});

		if !fired_keys.is_empty() {
			let bindings = self.config.keybindings.lock().unwrap().clone();
			for (key, behavior) in fired_keys {
				for keybinding in &bindings {
					if !keybinding.global && keybinding.key == key && keybinding.behavior == behavior {
						self.run_action(keybinding.action, ui, Some(keybinding.arguments.clone()));
					}
				}
			}
		}
	}

	pub fn record_keybinding(&mut self, ui: &Ui) -> Option<LocalKeybinding> {
		self.recording_keybinding?;

		let mut result = None;

		ui.input(|i| {
			for event in &i.events {
				if let Event::Key {
					key, pressed: true, modifiers, ..
				} = event
				{
					if matches!(
						key,
						Key::ControlLeft | Key::ShiftLeft | Key::ControlRight | Key::AltLeft | Key::AltRight | Key::SuperLeft | Key::SuperRight
					) {
						continue;
					}

					result = Some(LocalKeybinding { key: *key, modifiers: *modifiers });

					break;
				}
			}
		});

		if result.is_some() {
			self.recording_keybinding = None;
		}

		result
	}

	fn draw_volume_slider<Num: emath::Numeric + Into<f32>>(ui: &mut Ui, label_text: &str, value: &mut Num, range: impl Into<RangeInclusive<Num>> + Clone, theme: &Theme) {
		ui.horizontal(|ui| {
			ui.set_max_height(28.);
			ui.set_min_height(28.);
			ui.label(RichText::new(label_text).size(16.).color(theme.text()));
			let visuals = ui.visuals_mut();
			visuals.widgets.inactive.bg_fill = theme.background_bright();
			visuals.widgets.hovered.bg_fill = theme.text_bright();
			visuals.widgets.active.bg_fill = theme.text_bright();

			visuals.selection.bg_fill = theme.accent();

			ui.style_mut().spacing.slider_width = 160.0;
			ui.style_mut().spacing.slider_rail_height = 4.0;

			let slider_response = ui.add(Slider::new(value, range.clone()).show_value(false).handle_shape(HandleShape::Circle).trailing_fill(true));

			if slider_response.hovered() {
				ui.set_cursor_icon(CursorIcon::PointingHand);
			}

			ui.add_space(8.);
			let range = range.into();
			let value: f32 = (*value).into();
			ui.label(
				RichText::new(format!("{}%", (value / ((*range.end()).into() - (*range.start()).into()) * 100.) as usize))
					.size(14.)
					.color(if slider_response.hovered() { theme.accent() } else { theme.text_dim() }),
			);
		});
	}

	fn get_ring_radii(level: usize) -> (f32, f32) {
		let base_inner_radius = 60.0;
		let base_outer_radius = 150.0;
		let button_spacing = 8.0;
		let hover_expand = 10.0;
		let ring_thickness = base_outer_radius - base_inner_radius;

		if level == 0 {
			(base_inner_radius, base_outer_radius)
		} else {
			let inner_radius = base_outer_radius + (level as f32) * (button_spacing * 2.0 + hover_expand) + (level as f32 - 1.0) * ring_thickness;
			let outer_radius = inner_radius + ring_thickness;
			(inner_radius, outer_radius)
		}
	}

	fn run_action(&mut self, action: UiAction, ui: &mut Ui, arguments: Option<HashMap<String, KeybindingArgumentValue>>) {
		match action {
			UiAction::OpenSoundboard => {
				self.show(ui);
			},
			UiAction::ToggleSoundboard => {
				if self.visible {
					self.hide(ui);
				} else {
					self.show(ui);
				}
			},
			UiAction::CloseSoundboard => {
				self.hide(ui);
			},
			UiAction::PlaySound => {
				if let Some(arguments) = arguments
					&& let Some(argument) = arguments.get("Sound")
					&& let KeybindingArgumentValue::Uuid(id) = argument
					&& let Some(SoundTreeNode::Sound(sound)) = self.config.soundboard.get_sound_node(*id)
				{
					let _ = self.audio_player.lock().unwrap().play_user_sound(sound);
				}
			},
		}

		*self.keybinding_arguments.lock().unwrap() = None;
	}
}

impl eframe::App for App {
	fn clear_color(&self, _visuals: &Visuals) -> [f32; 4] {
		egui::Rgba::TRANSPARENT.to_array()
	}

	fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
		while let Ok(event) = self.rx.try_recv() {
			let arguments = self.keybinding_arguments.lock().unwrap().clone();
			self.run_action(event, ui, arguments);
		}

		self.listen_for_keybinding(ui);

		self.pick_file(ui);

		if self.visible {
			let overlay_frame = egui::Frame::NONE.fill(Color32::from_black_alpha(self.config.background_opacity.floor() as u8));
			self.draw_screen(ui, overlay_frame);
			if let Some(popup) = self.popup {
				match popup {
					Popup::DeleteSound => self.draw_delete_sound_popup(ui),
					Popup::DeletePage => self.draw_delete_page_popup(ui),
					Popup::DeleteGroup => self.draw_delete_group_popup(ui),
				}
			}

			if self.icon_picker_coords.is_some() {
				self.draw_icon_picker(ui);
			}

			if let Some(recording_id) = self.recording_keybinding
				&& let Some(recorded_binding) = self.record_keybinding(ui)
			{
				let mut keybindings = self.config.keybindings.lock().unwrap();
				let existing = keybindings.iter_mut().find(|kb| kb.id == recording_id);

				if let Some(binding) = existing {
					if binding.global
						&& let Some(old_hotkey) = binding.key.to_global()
					{
						let _ = self.hotkey_manager.unregister(old_hotkey);
					}

					binding.key = recorded_binding;

					if binding.global
						&& let Some(new_hotkey) = binding.key.to_global()
					{
						let _ = self.hotkey_manager.register(new_hotkey);
					}
				}
				self.is_recording.store(false, Ordering::Relaxed);
			}
		}
	}
}

/// Rotates a triangle clockwise around it's centroid by some angle.
///
/// # Parameters
///
/// - `vertices` - The vertices of the original triangle
/// - `angle` - The angle in radians to rotate clockwise
///
/// # Returns
///
/// The vertices of the rotated triangle
fn rotate_triangle_around_centroid(vertices: [Pos2; 3], angle: f32) -> Vec<Pos2> {
	let centroid = Pos2::new((vertices[0].x + vertices[1].x + vertices[2].x) / 3.0, (vertices[0].y + vertices[1].y + vertices[2].y) / 3.0);

	let rot = Rot2::from_angle(angle);

	vertices
		.iter()
		.map(|p| {
			let offset = *p - centroid;
			centroid + rot * offset
		})
		.collect()
}

/// Picks a random cute message for the upper left corner of the screen.
///
/// # Parameters
///
/// - `rng` - The shared `ThreadRng` instance for random generation
///
/// # Returns
///
/// The chosen message
fn random_cute_message(rng: &mut ThreadRng) -> &'static str {
	[
		"have a pawesome day :3",
		"youre doing great! :D",
		"do zacks dream of bloody sheep?",
		"nya!",
		"u are pawesomesauce!!! :D",
		"ruff :3",
		"you are loved :D",
		"time to play some stupid sounds >:3",
		"CALL ME I MISS U CALL ME",
		"ROT AND DIE!!!!!!!!!",
		"ur an idiot ^_^",
		"i can just really put whatever here n u have to see it huh",
		"shouts to grandma elaine",
		"b-baka!! -.-",
		"one more game ONE MORE GAME",
		"hhhhhhhhhhhhhhhhhhhh",
		"chatgpt which sound should i play",
		"made with alittle love n alotta hate",
		"hello zackaroni n cheese ^_^",
		"meow.......",
		"TIGHT TIGHT TIGHT BLUE YELLOW PINK",
		"bye squidward! byye squidward.......",
		"first i draw this head",
		"all bubble blowing babies will be beaten senseless by every able bodied patron in the bar",
		"but dont think we dont know how to WEEEEEEED em out!!",
		"its strange what desire will make foolish people do",
		"do i deserve this euphoric feeling?",
		"MOO!!!!!!!!!",
		"a stove.. is a stove..... no matter where you GOOOUUOOUOO",
		"i think there are some things that are understood only immediately or never",
		"\"when im really happy, i cant help but think about dying\"",
		"feeling like a passenger when all im meant to do is drive",
		"FUCKING LOSER !!!!!!!!!!!!",
		"im a chud im a chud im a fat little chud",
		"playing val all dayyy till the sun comes uppp!!!",
		"smoke more smoke more smoke more",
		"kills u",
		"BUY THAT OP N BLOW HIS SHIT SMOOVE OFF!!!!",
		"i luv u <3 :D",
		"dont do that DONT DOO THATTTT WE WERE HAVING A GOOD DAYYY",
		"i care about u so much sob",
	]
	.choose(rng)
	.unwrap()
}

fn empty_frame() -> eframe::egui::Frame {
	eframe::egui::Frame::NONE
}
