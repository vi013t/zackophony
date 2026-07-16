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
	egui::{self, Button, Color32, FontDefinitions, Painter, Pos2, Rect, RichText, Shape, Stroke, Ui, Vec2, ViewportBuilder, ViewportCommand, Visuals},
};
use global_hotkey::{
	GlobalHotKeyEvent,
	GlobalHotKeyManager,
	HotKeyState,
	hotkey::{Code, HotKey, Modifiers},
};
use rfd::FileDialog;
use uuid::Uuid;

use crate::{
	NAME,
	audio::{AudioPlayer, SoundData, UiSound},
	soundboard::{RadialButton, Soundboard},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
enum AppEvent {
	Toggle,
	Show,
	Hide,
}

pub struct App {
	visible: bool,

	_hotkey_manager: GlobalHotKeyManager,
	rx: Receiver<AppEvent>,

	audio_player: Arc<Mutex<AudioPlayer>>,
	soundboard: Arc<Mutex<Soundboard>>,
	hotkeys: Arc<Mutex<HashMap<HotKey, HotkeyAction>>>,

	selected_index: Option<(usize, usize)>,
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
		let hotkeys = HashMap::from([
			(HotKey::new(Some(Modifiers::SHIFT), Code::Backquote), HotkeyAction::Press(UiAction::ToggleSoundboard)),
			(HotKey::new(None, Code::Escape), HotkeyAction::Press(UiAction::CloseSoundboard)),
		]);

		for hotkey in hotkeys.keys() {
			hotkey_manager.register(*hotkey)?;
		}

		Ok(App {
			_hotkey_manager: hotkey_manager,
			hotkeys: Arc::new(Mutex::new(hotkeys)),
			soundboard: Arc::new(Mutex::new(soundboard)),
			audio_player: Arc::new(Mutex::new(audio_player)),
			rx,
			visible: false,
			last_index: None,
			selected_index: None,
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
						Button::new(RichText::new(egui_phosphor::regular::GEAR).size(25.).color(Color32::from_hex("#b4befe").unwrap()))
							.min_size(Vec2::splat(50.))
							.corner_radius(25.)
							.fill(Color32::from_hex("#11111bcc").unwrap()),
					);
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
			visuals.window_fill = Color32::from_hex("#11111b").unwrap();
			visuals.window_stroke = Stroke::new(1., Color32::from_hex("#b4befe").unwrap());

			{
				let sb = self.soundboard.lock().unwrap();
				let (name, icon) = match &sb.pages[self.page].sounds[*self.right_clicked_index.as_ref().unwrap()] {
					RadialButton::Sound(sound) => (&sound.name, &sound.icon),
					RadialButton::Folder(folder) => (&folder.name, &folder.icon),
				};

				if let Some(icon) = icon {
					ui.label(RichText::new(format!(" {}   {}", icon, name)).color(Color32::from_hex("#cdd6f4").unwrap()));
				} else {
					ui.label(format!(" {}", name));
				}
			}

			ui.separator();

			ui.add(
				Button::new(RichText::new(format!(" {}   Rename", egui_phosphor::regular::TEXTBOX)).color(Color32::from_hex("#cdd6f4").unwrap()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);
			ui.add(
				Button::new(RichText::new(format!(" {}   Change icon", egui_phosphor::regular::DIAMONDS_FOUR)).color(Color32::from_hex("#cdd6f4").unwrap()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);
			ui.add(
				Button::new(RichText::new(format!(" {}   Adjust volume for me", egui_phosphor::regular::SPEAKER_HIGH)).color(Color32::from_hex("#cdd6f4").unwrap()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);
			ui.add(
				Button::new(RichText::new(format!(" {}   Adjust volume for others", egui_phosphor::regular::SPEAKER_HIGH)).color(Color32::from_hex("#cdd6f4").unwrap()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);
			ui.add(
				Button::new(RichText::new(format!(" {}   Move to...", egui_phosphor::regular::FOLDER)).color(Color32::from_hex("#cdd6f4").unwrap()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);

			ui.separator();

			ui.add(
				Button::new(RichText::new(format!(" {}   Delete", egui_phosphor::regular::TRASH)).color(Color32::from_hex("#f38ba8").unwrap()))
					.min_size(Vec2::new(200., 25.))
					.fill(Color32::from_hex("#11111b").unwrap()),
			);
		});
		let painter = ui.painter_at(area_rect);

		if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
			let to_pointer = pointer_pos - screen_center;
			let distance = to_pointer.length();

			let ring_0_inner = base_inner_radius;
			let ring_0_outer = base_outer_radius;
			let ring_1_inner = base_outer_radius + button_spacing * 2.0;
			let ring_1_outer = ring_1_inner + (base_outer_radius - base_inner_radius);

			let mut detected_level = None;
			if distance >= ring_0_inner && distance <= ring_0_outer {
				detected_level = Some(0);
			} else if distance >= ring_1_inner && distance <= ring_1_outer {
				detected_level = Some(1);
			}

			if let Some(level) = detected_level {
				let mut angle = to_pointer.y.atan2(to_pointer.x);
				if angle < 0.0 {
					angle += TAU;
				}

				let slice_angle = TAU / num_items as f32;
				let adjusted_angle = (angle + slice_angle / 2.0) % TAU;
				let index = (adjusted_angle / slice_angle) as usize % num_items;

				self.selected_index = Some((level, index));
				let sb = self.soundboard.lock().unwrap();

				if response.clicked() {
					match level {
						0 => {
							if let RadialButton::Sound(sound) = &sb.pages[self.page].sounds[index] {
								let _ = self.audio_player.lock().unwrap().play_user_sound(&sound.name);
								self.visible = false;
								ui.send_viewport_cmd(ViewportCommand::Visible(false));
							}
						},
						1 => {
							if let RadialButton::Folder(folder) = &sb.pages[self.page].sounds[index] {
								let relative_index = index.saturating_sub(index);
								if let Some(sound) = folder.sounds.get(relative_index) {
									let _ = self.audio_player.lock().unwrap().play_user_sound(sound.name());
									self.visible = false;
									ui.send_viewport_cmd(ViewportCommand::Visible(false));
								}
							}
						},
						_ => {},
					}
				}
			} else {
				self.selected_index = None;
			}
		} else {
			self.selected_index = None;
		}

		if self.selected_index.is_some() {
			ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);

			if self.selected_index != self.last_index {
				let _ = self.audio_player.lock().unwrap().play_ui_sound(UiSound::Hover);
			}
		}

		self.last_index = self.selected_index;

		let slice_angle = TAU / num_items as f32;

		for sound_item_index in 0..num_items {
			let (name, icon, color) = {
				let sb = self.soundboard.lock().unwrap();
				match &sb.pages[self.page].sounds[sound_item_index] {
					RadialButton::Sound(sound) => (sound.name.clone(), sound.icon.clone(), Color32::from_hex("#b4befe").unwrap()),
					RadialButton::Folder(folder) => (folder.name.clone(), folder.icon.clone(), Color32::from_hex("#b5ebe2").unwrap()),
				}
			};

			let angle = (sound_item_index as f32) * slice_angle;
			self.draw_radial_button(
				ui,
				&painter,
				&name,
				icon.as_ref(),
				color,
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

		if let Some((_ring_index, item_index)) = &self.selected_index
			&& let RadialButton::Folder(folder) = &self.soundboard.lock().unwrap().pages[self.page].sounds[*item_index]
		{
			let slice_angle = 0.7;
			for index in 0..folder.sounds.len() {
				let start_angle = (*item_index as f32 + index as f32) * slice_angle;

				self.draw_radial_button(
					ui,
					&painter,
					folder.sounds[index].name(),
					folder.sounds[index].icon(),
					Color32::from_hex("#b4befe").unwrap(),
					screen_center,
					base_outer_radius + button_spacing * 2. + hover_expand,
					base_outer_radius + (base_outer_radius - base_inner_radius) + button_spacing * 2. + hover_expand,
					hover_expand,
					button_spacing,
					*item_index + index,
					slice_angle,
					start_angle + slice_angle / 2.,
					1,
				);
			}
		}
	}

	fn draw_radial_button(
		&self,
		ui: &mut Ui,
		painter: &Painter,
		name: &str,
		icon: Option<&String>,
		color: Color32,
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
		let fill_color = if is_hovered {
			Color32::from_hex("#1e1e2e").unwrap()
		} else {
			Color32::from_hex("#11111bcc").unwrap()
		};
		let gap = if is_hovered { 14. } else { 12. };

		let stroke_color = if is_hovered { Color32::from_hex("#f5c2e7").unwrap() } else { color };
		let text_color = if is_hovered { Color32::from_hex("#f5c2e7").unwrap() } else { color };

		let hover_t = ui.ctx().animate_bool(ui.id().with((level, sound_item_index)), is_hovered);
		let current_outer_radius = base_outer_radius + (hover_expand * hover_t);
		let current_inner_radius = base_inner_radius + (3.0 * hover_t);

		let outer_gap_angle = button_spacing / current_outer_radius;
		let inner_gap_angle = button_spacing / current_inner_radius;

		let start_angle_outer = angle - (slice_angle / 2.0) + outer_gap_angle;
		let end_angle_outer = angle + (slice_angle / 2.0) - outer_gap_angle;

		let start_angle_inner = angle - (slice_angle / 2.0) + inner_gap_angle;
		let end_angle_inner = angle + (slice_angle / 2.0) - inner_gap_angle;

		let mut points = Vec::new();
		let num_arc_points = 12; // High resolution for smooth curves

		for step in 0..=num_arc_points {
			let t = step as f32 / num_arc_points as f32;
			let a = start_angle_outer + (end_angle_outer - start_angle_outer) * t;
			points.push(screen_center + Vec2::new(a.cos(), a.sin()) * current_outer_radius);
		}

		for step in (0..=num_arc_points).rev() {
			let t = step as f32 / num_arc_points as f32;
			let a = start_angle_inner + (end_angle_inner - start_angle_inner) * t;
			points.push(screen_center + Vec2::new(a.cos(), a.sin()) * current_inner_radius);
		}

		self.draw_shadow(painter, &points);
		let fill_shape = Shape::convex_polygon(points.clone(), fill_color, Stroke::NONE);
		painter.add(fill_shape);
		painter.add(Shape::closed_line(points.clone(), Stroke::new(stroke_width, stroke_color)));

		let mid_radius = (current_inner_radius + current_outer_radius) / 2.0;
		let label_pos = screen_center + Vec2::new(angle.cos(), angle.sin()) * mid_radius;

		if let Some(icon) = icon {
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
				name,
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
}

impl eframe::App for App {
	fn clear_color(&self, _visuals: &Visuals) -> [f32; 4] {
		egui::Rgba::TRANSPARENT.to_array()
	}

	fn ui(&mut self, ctx: &mut Ui, _frame: &mut Frame) {
		while let Ok(event) = self.rx.try_recv() {
			match event {
				AppEvent::Show => {
					self.visible = true;
					ctx.send_viewport_cmd(ViewportCommand::Visible(true));
					// ctx.send_viewport_cmd(ViewportCommand::Focus);
				},
				AppEvent::Toggle => {
					if self.visible {
						self.visible = false;
						ctx.send_viewport_cmd(ViewportCommand::Visible(false));
					} else {
						self.visible = true;
						ctx.send_viewport_cmd(ViewportCommand::Visible(true));
						// ctx.send_viewport_cmd(ViewportCommand::Focus);
					}
				},
				AppEvent::Hide => {
					self.visible = false;
					ctx.send_viewport_cmd(ViewportCommand::Visible(false));
				},
			}
		}

		if self.visible {
			let overlay_frame = egui::Frame::NONE.fill(Color32::TRANSPARENT);

			egui::CentralPanel::default().frame(overlay_frame).show(ctx, |ui| {
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
								Button::new(RichText::new(egui_phosphor::regular::CARET_LEFT).color(Color32::from_hex("#b4befe").unwrap()).size(16.))
									.fill(Color32::from_hex("#11111bcc").unwrap())
									.min_size(Vec2::splat(30.))
									.corner_radius(15.),
							);

							if page_left.hovered() {
								ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
							}

							ui.add_space(100.);

							ui.label(
								RichText::new(format!("{} / {}", self.page + 1, self.soundboard.lock().unwrap().pages.len()))
									.color(Color32::from_hex("#b4befe").unwrap())
									.size(16.),
							);

							ui.add_space(100.);

							let page_right = ui.add(
								Button::new(RichText::new(egui_phosphor::regular::CARET_RIGHT).color(Color32::from_hex("#b4befe").unwrap()).size(16.))
									.fill(Color32::from_hex("#11111bcc").unwrap())
									.min_size(Vec2::splat(30.))
									.corner_radius(15.),
							);
							if page_right.hovered() {
								ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
							}
						});
					});

				self.draw_menu(ui);
			});
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
