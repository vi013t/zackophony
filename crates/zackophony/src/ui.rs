use std::{
	collections::HashMap,
	f32::consts::TAU,
	sync::{
		Arc,
		Mutex,
		mpmc::{Receiver, Sender, channel},
	},
};

use eframe::{
	Frame,
	Renderer,
	egui::{
		self,
		Align,
		Button,
		Color32,
		FontDefinitions,
		Layout,
		Painter,
		Pos2,
		Rect,
		RichText,
		Shadow,
		Shape,
		Stroke,
		Ui,
		UiBuilder,
		Vec2,
		ViewportBuilder,
		ViewportCommand,
		Visuals,
		vec2,
	},
};
use egui_phosphor::regular as icons;
use global_hotkey::{
	GlobalHotKeyEvent,
	GlobalHotKeyManager,
	HotKeyState,
	hotkey::{Code, HotKey, Modifiers},
};
use rfd::FileDialog;
use rodio::DeviceTrait;
use uuid::Uuid;

use crate::{
	NAME,
	audio::{AudioPlayer, SoundData, UiSound},
	soundboard::{Preferences, SoundTreeNode, Soundboard},
};

#[derive(Debug, Clone, PartialEq, Eq, Copy, Hash)]
enum SettingsTab {
	Appearance,
	Hotkeys,
	System,
	Sounds,
	Support,
}

enum AppScreen {
	Soundboard,
	Settings(SettingsTab),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
enum AppEvent {
	Toggle,
	Show,
	Hide,
}

pub struct App {
	visible: bool,
	screen: AppScreen,

	_hotkey_manager: GlobalHotKeyManager,
	rx: Receiver<AppEvent>,

	audio_player: Arc<Mutex<AudioPlayer>>,
	soundboard: Arc<Mutex<Soundboard>>,
	hotkeys: Arc<Mutex<HashMap<HotKey, HotkeyAction>>>,
	preferences: Preferences,

	selected_index: Option<(usize, usize)>,
	active_folder: Option<usize>,
	last_index: Option<(usize, usize)>,
	right_clicked_index: Option<usize>,
	page: usize,
}

impl App {
	fn new(rx: Receiver<AppEvent>) -> anyhow::Result<App> {
		let soundboard = Soundboard::new()?;
		let mut audio_player = AudioPlayer::new()?;
		for sound in soundboard.sounds() {
			audio_player.upload_user_sound(&sound.name, sound.data.clone())?;
		}

		let hotkey_manager = GlobalHotKeyManager::new()?;
		let hotkeys = HashMap::from([(HotKey::new(Some(Modifiers::SHIFT), Code::Backquote), HotkeyAction::Press(UiAction::ToggleSoundboard))]);

		for hotkey in hotkeys.keys() {
			hotkey_manager.register(*hotkey)?;
		}

		Ok(App {
			_hotkey_manager: hotkey_manager,
			hotkeys: Arc::new(Mutex::new(hotkeys)),
			screen: AppScreen::Soundboard,
			soundboard: Arc::new(Mutex::new(soundboard)),
			preferences: Preferences::default(),
			audio_player: Arc::new(Mutex::new(audio_player)),
			rx,
			visible: false,
			last_index: None,
			selected_index: None,
			active_folder: None,
			right_clicked_index: None,
			page: 0,
		})
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
				.with_maximized(true),
			renderer: Renderer::Glow,
			..Default::default()
		};

		eframe::run_native(
			NAME,
			options,
			Box::new(move |cc| {
				let ctx = cc.egui_ctx.clone();

				let mut fonts = FontDefinitions::default();
				egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
				cc.egui_ctx.set_fonts(fonts);

				let audio_player = app.audio_player.clone();
				let soundboard = app.soundboard.clone();
				let hotkeys = app.hotkeys.clone();

				std::thread::spawn(move || {
					let receiver = GlobalHotKeyEvent::receiver();
					let _ = tx.send(AppEvent::Hide);
					loop {
						if let Ok(event) = receiver.recv() {
							for (key, action) in hotkeys.lock().unwrap().iter() {
								if key.id() == event.id() {
									match action {
										HotkeyAction::Hold { on_press, on_release } => {
											if event.state == HotKeyState::Pressed {
												on_press.run(soundboard.clone(), audio_player.clone(), &tx);
											} else if event.state == HotKeyState::Released {
												on_release.run(soundboard.clone(), audio_player.clone(), &tx);
											}
										},

										HotkeyAction::Press(ui_action) => {
											if event.state == HotKeyState::Pressed {
												ui_action.run(soundboard.clone(), audio_player.clone(), &tx);
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

	fn get_wedge_polygon(screen_center: Pos2, inner_radius: f32, outer_radius: f32, button_spacing: f32, slice_angle: f32, angle: f32) -> Vec<Pos2> {
		let outer_gap_angle = button_spacing / outer_radius.max(1.0);
		let inner_gap_angle = button_spacing / inner_radius.max(1.0);

		let start_angle_outer = angle - (slice_angle / 2.0) + outer_gap_angle;
		let end_angle_outer = angle + (slice_angle / 2.0) - outer_gap_angle;

		let start_angle_inner = angle - (slice_angle / 2.0) + inner_gap_angle;
		let end_angle_inner = angle + (slice_angle / 2.0) - inner_gap_angle;

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

	fn draw_config_buttons(&mut self, ui: &mut Ui) {
		egui::Area::new("bottom_right_panel".into())
			.anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-10.0, -10.0))
			.show(ui, |ui| {
				ui.vertical(|ui| {
					let upload_button = ui.add(
						Button::new(RichText::new("+").size(25.))
							.min_size(Vec2::splat(50.))
							.corner_radius(25.)
							.fill(Color32::from_hex("#11111bcc").unwrap()),
					);

					if upload_button.clicked()
						&& let Some(path) = FileDialog::new().add_filter("Audio files", &["mp3"]).pick_file()
					{
						let _ = self.audio_player.lock().unwrap().upload_user_sound(
							#[allow(clippy::unnecessary_to_owned, reason = "false positive; will error without this")]
							&path.file_name().unwrap().to_str().unwrap().to_string(),
							SoundData::new(std::fs::read(path).unwrap()).unwrap(),
						);
					}

					ui.add_space(10.);

					let settings_button = ui.add(
						Button::new(RichText::new(icons::GEAR).size(25.).color(Color32::from_hex("#b4befe").unwrap()))
							.min_size(Vec2::splat(50.))
							.corner_radius(25.)
							.fill(Color32::from_hex("#11111bcc").unwrap()),
					);

					if settings_button.clicked() {
						self.screen = AppScreen::Settings(SettingsTab::Appearance);
					}
				});
			});
	}

	fn draw_menu(&mut self, ui: &mut Ui) {
		let base_outer_radius = 150.0;
		let base_inner_radius = 60.0;
		let hover_expand = 10.0;
		let button_spacing = 8.0;

		let num_items = self.soundboard.lock().unwrap().pages[self.page].sounds.len();

		if num_items == 0 {
			return;
		}

		let screen_center = ui.ctx().viewport_rect().center();
		let max_radius = 9999999999.;

		let area_rect = Rect::from_center_size(screen_center, Vec2::splat(max_radius * 2.2));
		let response = ui.allocate_rect(area_rect, egui::Sense::click_and_drag());

		if response.secondary_clicked() {
			self.right_clicked_index = self.selected_index.map(|i| i.1);
		}

		response.context_menu(|ui| {
			let visuals = ui.visuals_mut();
			visuals.window_fill = self.preferences.theme.sound_background();
			visuals.window_stroke = Stroke::new(1., self.preferences.theme.sound_border());

			{
				let sb = self.soundboard.lock().unwrap();
				let index = self.right_clicked_index.unwrap_or(0);

				if let Some(sound_node) = sb.pages[self.page].sounds.get(index) {
					let (name, icon) = match sound_node {
						SoundTreeNode::Sound(sound) => (&sound.name, &sound.icon),
						SoundTreeNode::Group(folder) => (&folder.name, &folder.icon),
					};

					if let Some(icon) = icon {
						ui.label(RichText::new(format!(" {}   {}", icon, name)).color(Color32::from_hex("#cdd6f4").unwrap()));
					} else {
						ui.label(format!(" {}", name));
					}
				}
			}

			ui.separator();

			ui.add(
				Button::new(RichText::new(format!(" {}   Rename", icons::TEXTBOX)).color(Color32::from_hex("#cdd6f4").unwrap()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);
			ui.add(
				Button::new(RichText::new(format!(" {}   Change icon", icons::DIAMONDS_FOUR)).color(Color32::from_hex("#cdd6f4").unwrap()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);
			ui.add(
				Button::new(RichText::new(format!(" {}   Adjust volume for me", icons::SPEAKER_HIGH)).color(Color32::from_hex("#cdd6f4").unwrap()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);
			ui.add(
				Button::new(RichText::new(format!(" {}   Adjust volume for others", icons::SPEAKER_HIGH)).color(Color32::from_hex("#cdd6f4").unwrap()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);
			ui.add(
				Button::new(RichText::new(format!(" {}   Move to...", icons::FOLDER)).color(Color32::from_hex("#cdd6f4").unwrap()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);

			ui.separator();

			ui.add(
				Button::new(RichText::new(format!(" {}   Delete", icons::TRASH)).color(self.preferences.theme.red()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);
		});
		let painter = ui.painter_at(area_rect);

		let slice_angle = TAU / num_items as f32;
		let slice_angle_l1 = 0.7;

		let mut new_selected_index = None;
		let mut new_active_folder = self.active_folder;

		if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
			let sb = self.soundboard.lock().unwrap();

			// check level 1 hits first (foreground elements)
			if let Some(folder_idx) = self.active_folder
				&& let Some(SoundTreeNode::Group(folder)) = sb.pages[self.page].sounds.get(folder_idx)
			{
				for sub_idx in 0..folder.sounds.len() {
					let start_angle = (folder_idx as f32 + sub_idx as f32) * slice_angle_l1;
					let angle_l1 = start_angle + slice_angle_l1 / 2.0;

					let inner_r = base_outer_radius + button_spacing * 2.0 + hover_expand;
					let outer_r = base_outer_radius + (base_outer_radius - base_inner_radius) + button_spacing * 2.0 + hover_expand;

					let points = Self::get_wedge_polygon(screen_center, inner_r, outer_r, button_spacing, slice_angle_l1, angle_l1);

					if Self::point_in_polygon(pointer_pos, &points) {
						new_selected_index = Some((1, sub_idx));
						break;
					}
				}
			}

			// check level 0 hits if not hovering a level 1 item
			if new_selected_index.is_none() {
				for i in 0..num_items {
					let angle = (i as f32) * slice_angle;

					let points = Self::get_wedge_polygon(screen_center, base_inner_radius, base_outer_radius, button_spacing, slice_angle, angle);

					if Self::point_in_polygon(pointer_pos, &points) {
						new_selected_index = Some((0, i));

						if let SoundTreeNode::Group(_) = &sb.pages[self.page].sounds[i] {
							new_active_folder = Some(i);
						} else {
							new_active_folder = None; // Reset folder if hovering a regular sound
						}
						break;
					}
				}
			}

			if response.clicked()
				&& let Some((level, idx)) = new_selected_index
			{
				match level {
					0 => {
						if let SoundTreeNode::Sound(sound) = &sb.pages[self.page].sounds[idx] {
							let _ = self.audio_player.lock().unwrap().play_user_sound(&sound.name);
							self.visible = false;
							ui.send_viewport_cmd(ViewportCommand::Visible(false));
						}
					},
					1 => {
						if let Some(folder_idx) = self.active_folder
							&& let SoundTreeNode::Group(folder) = &sb.pages[self.page].sounds[folder_idx]
							&& let Some(sound) = folder.sounds.get(idx)
						{
							let _ = self.audio_player.lock().unwrap().play_user_sound(sound.name());
							self.visible = false;
							ui.send_viewport_cmd(ViewportCommand::Visible(false));
						}
					},
					_ => {},
				}
			}
		}

		self.selected_index = new_selected_index;
		self.active_folder = new_active_folder;

		if self.selected_index.is_some() {
			ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);

			if self.selected_index != self.last_index {
				let _ = self.audio_player.lock().unwrap().play_ui_sound(UiSound::Hover);
			}
		}

		self.last_index = self.selected_index;

		// level 0
		for sound_item_index in 0..num_items {
			let (name, icon, color) = {
				let sb = self.soundboard.lock().unwrap();
				match &sb.pages[self.page].sounds[sound_item_index] {
					SoundTreeNode::Sound(sound) => (sound.name.clone(), sound.icon.clone(), self.preferences.theme.sound_border()),
					SoundTreeNode::Group(folder) => (folder.name.clone(), folder.icon.clone(), self.preferences.theme.sound_group_border()),
				}
			};

			let angle = (sound_item_index as f32) * slice_angle;
			let node = &self.soundboard.lock().unwrap().pages[self.page].sounds[sound_item_index];
			self.draw_radial_button(
				ui,
				&painter,
				node,
				screen_center,
				base_inner_radius,
				base_outer_radius,
				hover_expand,
				button_spacing,
				sound_item_index,
				slice_angle,
				angle,
				0,
			);
		}

		// level 1
		if let Some(folder_idx) = self.active_folder {
			let sb = self.soundboard.lock().unwrap();
			if let Some(SoundTreeNode::Group(folder)) = sb.pages[self.page].sounds.get(folder_idx) {
				for index in 0..folder.sounds.len() {
					let start_angle = (folder_idx as f32 + index as f32) * slice_angle_l1;
					let node = &folder.sounds[index];

					self.draw_radial_button(
						ui,
						&painter,
						node,
						screen_center,
						base_outer_radius + button_spacing * 2. + hover_expand,
						base_outer_radius + (base_outer_radius - base_inner_radius) + button_spacing * 2. + hover_expand,
						hover_expand,
						button_spacing,
						index,
						slice_angle_l1,
						start_angle + slice_angle_l1 / 2.,
						1,
					);
				}
			}
		}
	}

	fn draw_radial_button(
		&self,
		ui: &mut Ui,
		painter: &Painter,
		sound_node: &SoundTreeNode,
		screen_center: Pos2,
		base_inner_radius: f32,
		base_outer_radius: f32,
		hover_expand: f32,
		button_spacing: f32,
		sound_item_index: usize,
		slice_angle: f32,
		angle: f32,
		level: usize,
	) {
		let is_hovered = self.selected_index == Some((level, sound_item_index));

		let stroke_width = if is_hovered { 5.0 } else { 2.0 };
		let gap = if is_hovered { 14. } else { 12. };

		let fill_color = sound_node.background_color(self.preferences.theme, is_hovered);
		let stroke_color = sound_node.border_color(self.preferences.theme, is_hovered);
		let text_color = stroke_color;

		let hover_t = ui.ctx().animate_bool(ui.id().with((level, sound_item_index)), is_hovered);
		let current_outer_radius = base_outer_radius + (hover_expand * hover_t);
		let current_inner_radius = base_inner_radius + (3.0 * hover_t);

		let points = Self::get_wedge_polygon(screen_center, current_inner_radius, current_outer_radius, button_spacing, slice_angle, angle);

		self.draw_shadow(painter, &points);
		let fill_shape = Shape::convex_polygon(points.clone(), fill_color, Stroke::NONE);
		painter.add(fill_shape);
		painter.add(Shape::closed_line(points, Stroke::new(stroke_width, stroke_color)));

		let mid_radius = (current_inner_radius + current_outer_radius) / 2.0;
		let label_pos = screen_center + Vec2::new(angle.cos(), angle.sin()) * mid_radius;

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
			let size = parent.size() * 0.5;
			let child = Rect::from_center_size(parent.center(), size);

			ui.scope_builder(UiBuilder::new().max_rect(child), |ui| {
				eframe::egui::Frame::default()
					.fill(self.preferences.theme.background())
					.inner_margin(16.)
					.corner_radius(8.)
					.stroke(Stroke::new(1., Color32::from_hex("#45475a").unwrap()))
					.shadow(Shadow::default())
					.show(ui, |ui| {
						ui.horizontal(|ui| {
							ui.label(
								RichText::new(format!(" {}   {NAME} Settings", icons::GEAR))
									.size(16.)
									.color(self.preferences.theme.text_bright()),
							);
							ui.add_space(ui.available_width());
							ui.visuals_mut().widgets.hovered = ui.visuals().widgets.inactive;
							ui.visuals_mut().widgets.active = ui.visuals().widgets.inactive;
							let close_button = ui.add(
								Button::new(RichText::new(icons::X).color(self.preferences.theme.red()))
									.fill(Color32::TRANSPARENT)
									.stroke(Stroke::NONE),
							);

							if close_button.hovered() {
								ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
							}

							if close_button.clicked() {
								self.screen = AppScreen::Soundboard;
							}
						});
						ui.add_space(8.);
						ui.visuals_mut().widgets.noninteractive.bg_stroke.color = Color32::from_hex("#585b70").unwrap();
						ui.separator();
						ui.add_space(8.);

						ui.horizontal(|ui| {
							ui.allocate_ui_with_layout(vec2(170., ui.available_height()), Layout::top_down(Align::Max), |ui| {
								ui.vertical(|ui| {
									ui.visuals_mut().widgets.hovered = ui.visuals().widgets.inactive;
									ui.visuals_mut().widgets.active = ui.visuals().widgets.inactive;

									let appearance_button = ui.add(
										Button::new(
											RichText::new(format!(" {}   Appearance", icons::PALETTE))
												.color(self.preferences.theme.text_bright())
												.size(16.),
										)
										.fill(Color32::TRANSPARENT)
										.stroke(Stroke::NONE),
									);

									if appearance_button.hovered() {
										ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
									}

									if appearance_button.clicked() {
										self.screen = AppScreen::Settings(SettingsTab::Appearance);
									}

									ui.add_space(8.);

									let sounds_button = ui.add(
										Button::new(
											RichText::new(format!(" {}   Sounds", icons::SPEAKER_HIGH))
												.color(self.preferences.theme.text_bright())
												.size(16.),
										)
										.fill(Color32::TRANSPARENT)
										.stroke(Stroke::NONE),
									);
									if sounds_button.hovered() {
										ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
									}

									if sounds_button.clicked() {
										self.screen = AppScreen::Settings(SettingsTab::Sounds);
									}

									ui.add_space(8.);

									let system_button = ui.add(
										Button::new(RichText::new(format!(" {}   System", icons::CPU)).color(self.preferences.theme.text_bright()).size(16.))
											.fill(Color32::TRANSPARENT)
											.stroke(Stroke::NONE),
									);
									if system_button.hovered() {
										ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
									}

									if system_button.clicked() {
										self.screen = AppScreen::Settings(SettingsTab::System);
									}

									ui.add_space(8.);

									let hotkeys_button = ui.add(
										Button::new(
											RichText::new(format!(" {}   Hotkeys", icons::KEYBOARD))
												.color(self.preferences.theme.text_bright())
												.size(16.),
										)
										.fill(Color32::TRANSPARENT)
										.stroke(Stroke::NONE),
									);

									if hotkeys_button.hovered() {
										ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
									}

									if hotkeys_button.clicked() {
										self.screen = AppScreen::Settings(SettingsTab::Hotkeys);
									}

									ui.add_space(8.);

									let support_button = ui.add(
										Button::new(
											RichText::new(format!(" {}   Support", icons::QUESTION))
												.color(self.preferences.theme.text_bright())
												.size(16.),
										)
										.fill(Color32::TRANSPARENT)
										.stroke(Stroke::NONE),
									);

									if support_button.hovered() {
										ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
									}

									if support_button.clicked() {
										self.screen = AppScreen::Settings(SettingsTab::Support);
									}

									ui.add_space(ui.available_height());
								});
							});

							ui.separator();

							match tab {
								SettingsTab::Appearance => {
									ui.columns(2, |ui| {
										ui[0].vertical(|ui| {
											ui.label(RichText::new("Theme"));
											ui.label(RichText::new("Overlay opacity"));
										});
									});
								},
								SettingsTab::Hotkeys => {
									ui.vertical(|ui| {
										for (hotkey, action) in self.hotkeys.lock().unwrap().iter() {
											ui.horizontal(|ui| {
												ui.label("Key");
												ui.label(hotkey.to_string());
												ui.label("Action");
											});
										}
									});
								},
								SettingsTab::System => {
									ui.columns(2, |ui| {
										ui[0].vertical(|ui| {
											ui.label(RichText::new("Output device:").color(self.preferences.theme.text()).size(16.));
											ui.add_space(4.);
											ui.label(RichText::new("Input device:").color(self.preferences.theme.text()).size(16.));
											ui.add_space(4.);
											ui.label(RichText::new("Virtual cable device:").color(self.preferences.theme.text()).size(16.));
										});

										ui[1].vertical(|ui| {
											egui::ComboBox::new("output-devices", "").show_ui(ui, |ui| {
												for output_device in AudioPlayer::output_devices() {
													ui.label(output_device.description().unwrap().extended().join(", "));
												}
											});
											egui::ComboBox::new("input-devices", "").show_ui(ui, |ui| {
												for input_device in AudioPlayer::input_devices() {
													ui.label(input_device.description().unwrap().extended().join(", "));
												}
											});
											egui::ComboBox::new("vb-cable-device", "").show_ui(ui, |ui| {
												for output_device in AudioPlayer::output_devices() {
													ui.label(output_device.description().unwrap().extended().join(", "));
												}
											});
										});
									});
								},
								SettingsTab::Sounds => {},
								SettingsTab::Support => {
									ui.horizontal_wrapped(|ui| {
										ui.label(
											RichText::new("zack i made the fucking app why are you looking in the support tab just send me a message you fucking idiot")
												.size(16.)
												.color(self.preferences.theme.text()),
										);
									});
									ui.add_space(ui.available_height());
								},
							}
						});

						ui.add_space(ui.available_height());
					});
			});
		});
	}

	fn draw_soundboard(&mut self, ui: &mut Ui, overlay_frame: eframe::egui::Frame) {
		egui::CentralPanel::default().frame(overlay_frame).show(ui, |ui| {
			self.draw_config_buttons(ui);
			egui::Area::new("page_name".into())
				.anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -200.0))
				.show(ui, |ui| {
					ui.horizontal(|ui| {
						let mut page_name = self.soundboard.lock().unwrap().pages[self.page].name.clone();
						if page_name.is_empty() {
							page_name = format!("Page {}", self.page + 1);
						}
						ui.label(RichText::new(page_name).color(Color32::from_hex("#b4befe").unwrap()));
					});
				});

			egui::Area::new("page_selector".into())
				.anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 200.0))
				.show(ui, |ui| {
					ui.horizontal(|ui| {
						ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::NONE;
						let page_left = ui.add(
							Button::new(RichText::new(icons::CARET_LEFT).color(Color32::from_hex("#b4befe").unwrap()).size(16.))
								.fill(Color32::from_hex("#11111bcc").unwrap())
								.min_size(Vec2::splat(30.))
								.corner_radius(15.),
						);

						if page_left.hovered() {
							ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
						}

						if page_left.clicked() && self.page > 0 {
							self.page -= 1;
						}

						ui.add_space(100.);

						ui.label(
							RichText::new(format!("{} / {}", self.page + 1, self.soundboard.lock().unwrap().pages.len()))
								.color(Color32::from_hex("#b4befe").unwrap())
								.size(16.),
						);

						ui.add_space(100.);

						let page_right = ui.add(
							Button::new(RichText::new(icons::CARET_RIGHT).color(Color32::from_hex("#b4befe").unwrap()).size(16.))
								.fill(Color32::from_hex("#11111bcc").unwrap())
								.min_size(Vec2::splat(30.))
								.corner_radius(15.),
						);

						if page_right.hovered() {
							ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
						}

						if page_right.clicked() && self.page < self.soundboard.lock().unwrap().pages.len() - 1 {
							self.page += 1;
						}
					});
				});

			self.draw_menu(ui);
		});
	}
}

impl eframe::App for App {
	fn clear_color(&self, _visuals: &Visuals) -> [f32; 4] {
		egui::Rgba::TRANSPARENT.to_array()
	}

	fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
		while let Ok(event) = self.rx.try_recv() {
			match event {
				AppEvent::Show => {
					self.visible = true;
					ui.send_viewport_cmd(ViewportCommand::Visible(true));
				},
				AppEvent::Toggle => {
					if self.visible {
						self.visible = false;
						ui.send_viewport_cmd(ViewportCommand::Visible(false));
					} else {
						self.visible = true;
						ui.send_viewport_cmd(ViewportCommand::Visible(true));
					}
				},
				AppEvent::Hide => {
					self.visible = false;
					ui.send_viewport_cmd(ViewportCommand::Visible(false));
				},
			}
		}

		if self.visible {
			let overlay_frame = egui::Frame::NONE.fill(Color32::from_black_alpha(100));
			self.draw_screen(ui, overlay_frame);
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UiAction {
	OpenSoundboard,
	CloseSoundboard,
	ToggleSoundboard,
	OpenSettings,
	CloseSettings,
	PlaySound { sound_id: Uuid },
}

impl UiAction {
	fn run(&self, soundboard: Arc<Mutex<Soundboard>>, audio_player: Arc<Mutex<AudioPlayer>>, tx: &Sender<AppEvent>) {
		match self {
			UiAction::OpenSoundboard => {
				let _ = tx.send(AppEvent::Show);
			},
			UiAction::CloseSoundboard => {
				let _ = tx.send(AppEvent::Hide);
			},
			UiAction::ToggleSoundboard => {
				let _ = tx.send(AppEvent::Toggle);
			},
			UiAction::OpenSettings => todo!(),
			UiAction::CloseSettings => todo!(),
			UiAction::PlaySound { sound_id } => todo!(),
		}
	}
}

pub enum HotkeyAction {
	Hold { on_press: UiAction, on_release: UiAction },
	Press(UiAction),
}
