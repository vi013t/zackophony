use std::{
	f32::consts::TAU,
	sync::{
		Arc,
		Mutex,
		mpmc::{Receiver, channel},
	},
};

use eframe::{
	Frame,
	Renderer,
	egui::{self, Button, Color32, FontDefinitions, Rect, RichText, Shape, Stroke, Ui, Vec2, ViewportBuilder, ViewportCommand, Visuals},
};
use global_hotkey::{
	GlobalHotKeyEvent,
	GlobalHotKeyManager,
	HotKeyState,
	hotkey::{Code, HotKey, Modifiers},
};
use rfd::FileDialog;

use crate::{
	NAME,
	audio::{AudioPlayer, SoundData, UiSound},
	soundboard::{RadialButton, Soundboard},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
enum AppEvent {
	Show,
	Hide,
}

pub struct App {
	visible: bool,

	hotkey_manager: GlobalHotKeyManager,
	rx: Receiver<AppEvent>,

	audio_player: Arc<Mutex<AudioPlayer>>,
	soundboard: Arc<Mutex<Soundboard>>,

	selected_index: Option<usize>,
	last_index: Option<usize>,
	page: usize,
}

impl App {
	fn new(rx: Receiver<AppEvent>) -> anyhow::Result<App> {
		let soundboard = Soundboard::new()?;
		let mut audio_player = AudioPlayer::new()?;
		for sound in soundboard.sounds() {
			audio_player.upload_user_sound(&sound.name, sound.data.clone())?;
		}

		Ok(App {
			hotkey_manager: GlobalHotKeyManager::new()?,
			soundboard: Arc::new(Mutex::new(soundboard)),
			audio_player: Arc::new(Mutex::new(audio_player)),
			rx,
			visible: false,
			last_index: None,
			selected_index: None,
			page: 0,
		})
	}

	pub fn start() -> anyhow::Result<()> {
		let (tx, rx) = channel();
		let app = App::new(rx)?;
		let hotkey = HotKey::new(Some(Modifiers::SHIFT), Code::Backquote);
		app.hotkey_manager.register(hotkey)?;

		let options = eframe::NativeOptions {
			viewport: ViewportBuilder::default()
                .with_title(NAME)
                .with_transparent(true)
                .with_decorations(false)
                // .with_always_on_top()
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

				std::thread::spawn(move || {
					let receiver = GlobalHotKeyEvent::receiver();
					let _ = tx.send(AppEvent::Hide);
					loop {
						if let Ok(event) = receiver.recv() {
							if event.state == HotKeyState::Pressed {
								let _ = tx.send(AppEvent::Show);
							} else if event.state == HotKeyState::Released {
								if app.visible
									&& let Some(index) = app.selected_index.as_ref()
								{
									let RadialButton::Sound(sound) = &soundboard.lock().unwrap().pages[app.page].sounds[*index] else {
										unreachable!()
									};
									let _ = audio_player.lock().unwrap().play_user_sound(&sound.name);
								}
								let _ = tx.send(AppEvent::Hide);
							}

							ctx.request_repaint();
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
							.fill(Color32::from_hex("#1e1e2e").unwrap())
							.stroke(Stroke::new(3.0, Color32::from_hex("#b4befe").unwrap())),
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

		let max_radius = base_outer_radius + hover_expand;
		let area_rect = Rect::from_center_size(screen_center, Vec2::splat(max_radius * 2.2));
		let response = ui.allocate_rect(area_rect, egui::Sense::click_and_drag());
		let painter = ui.painter_at(area_rect);

		if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
			let to_pointer = pointer_pos - screen_center;
			let distance = to_pointer.length();

			if distance >= base_inner_radius && distance <= max_radius {
				let mut angle = to_pointer.y.atan2(to_pointer.x);
				if angle < 0.0 {
					angle += TAU;
				}

				let slice_angle = TAU / num_items as f32;
				let adjusted_angle = (angle + slice_angle / 2.0) % TAU;
				let index = (adjusted_angle / slice_angle) as usize % num_items;

				self.selected_index = Some(index);
				let sb = self.soundboard.lock().unwrap();

				if response.clicked() {
					self.visible = false;
					let sound = match &sb.pages[self.page].sounds[*self.selected_index.as_ref().unwrap()] {
						RadialButton::Sound(sound) => sound,
						RadialButton::Folder(_sound_folder) => unreachable!(),
					};

					let _ = self.audio_player.lock().unwrap().play_user_sound(&sound.name);
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

		for i in 0..num_items {
			let angle = (i as f32) * slice_angle;
			let is_hovered = self.selected_index == Some(i);

			let stroke_width = if is_hovered { 5.0 } else { 2.0 };
			let fill_color = if is_hovered {
				Color32::from_hex("#1e1e2e").unwrap()
			} else {
				Color32::from_hex("#11111bcc").unwrap()
			};
			let gap = if is_hovered { 14. } else { 12. };

			let stroke_color = if is_hovered {
				Color32::from_hex("#f5c2e7").unwrap()
			} else {
				Color32::from_hex("#b4befe").unwrap()
			};

			let text_color = if is_hovered {
				Color32::from_hex("#f5c2e7").unwrap()
			} else {
				Color32::from_hex("#b4befe").unwrap()
			};

			let hover_t = ui.ctx().animate_bool(ui.id().with(i), is_hovered);
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

			self.paint_soft_shadow(&painter, &points);

			let fill_shape = Shape::convex_polygon(points.clone(), fill_color, Stroke::NONE);

			painter.add(fill_shape);

			painter.add(Shape::closed_line(points.clone(), Stroke::new(stroke_width, stroke_color)));

			let mid_radius = (current_inner_radius + current_outer_radius) / 2.0;
			let label_pos = screen_center + Vec2::new(angle.cos(), angle.sin()) * mid_radius;

			let sb = self.soundboard.lock().unwrap();
			let sound = match &sb.pages[self.page].sounds[i] {
				RadialButton::Sound(sound) => sound,
				_ => unreachable!(),
			};

			if let Some(icon) = &sound.icon {
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
					&sound.name,
					egui::FontId::proportional(12.0 + (2.0 * hover_t)),
					text_color,
				);
			}
		}
	}

	fn paint_soft_shadow(&self, painter: &egui::Painter, points: &[egui::Pos2]) {
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
					ctx.send_viewport_cmd(ViewportCommand::Focus);
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
				ui.vertical_centered(|ui| {
					{
						let sb = self.soundboard.lock().unwrap();
						let mut page_name = sb.pages[self.page].name.clone();
						if page_name.is_empty() {
							page_name = format!("Page {}", self.page + 1);
						}
						ui.label(RichText::new(page_name).color(Color32::from_hex("#b4befe").unwrap()).size(16.));
					}
					self.draw_menu(ui);
					ui.horizontal(|ui| {
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
			});
		}
	}
}
