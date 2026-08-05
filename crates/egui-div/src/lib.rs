use std::f32::consts::{FRAC_PI_2, PI};

use derive_setters::Setters;
use eframe::egui::{
	Align,
	Align2,
	Color32,
	CursorIcon,
	FontId,
	Layout,
	Margin,
	Mesh,
	Pos2,
	Rect,
	Response,
	Sense,
	Shadow,
	Shape,
	Stroke,
	Ui,
	UiBuilder,
	Vec2,
	Widget,
	lerp,
	pos2,
	vec2,
};
use try_as::macros::From;
use uuid::Uuid;

#[derive(Default, Clone, Copy)]
#[allow(clippy::enum_variant_names, reason = "readability")]
pub enum GradientDirection {
	ToLeft,
	ToRight,
	ToTop,
	ToTopLeft,
	ToTopRight,
	ToBottom,
	ToBottomLeft,

	#[default]
	ToBottomRight,
}

#[derive(From)]
pub enum Background {
	Solid(Color32),
	Gradient(Gradient),
}

#[derive(Setters)]
pub struct Gradient {
	color1: Color32,
	color2: Color32,
	direction: GradientDirection,
}

impl Gradient {
	pub fn new() -> Gradient {
		Gradient::default()
	}
}

impl Default for Gradient {
	fn default() -> Self {
		Gradient {
			color1: Color32::WHITE,
			color2: Color32::WHITE,
			direction: GradientDirection::ToBottom,
		}
	}
}

#[derive(Setters)]
pub struct Div {
	#[setters(into)]
	size: Vec2,

	#[setters(into)]
	fill: Background,
	corner_radius: f32,
	#[setters(strip_option)]
	shadow: Option<Shadow>,
	stroke: Stroke,
	#[setters(into)]
	inner_margin: Margin,
	cursor: CursorIcon,

	hovered: Hovered,

	#[setters(strip_option, into)]
	text: Option<String>,
	text_color: Color32,
	font_size: f32,
	text_align: Align2,

	id: Uuid,
}

#[derive(Setters)]
pub struct Hovered {
	#[setters(strip_option)]
	fill: Option<Background>,
	scale: f32,
	text: Option<String>,
}

impl Default for Hovered {
	fn default() -> Self {
		Self {
			scale: 1.,
			fill: None,
			text: None,
		}
	}
}

impl Default for Div {
	fn default() -> Self {
		Self {
			fill: Background::Solid(Color32::TRANSPARENT),
			text_color: Color32::BLACK,
			text: None,
			size: vec2(50., 20.),
			corner_radius: 0.,
			shadow: None,
			id: Uuid::new_v4(),
			stroke: Stroke::NONE,
			inner_margin: Margin::default(),
			cursor: CursorIcon::default(),
			hovered: Hovered::default(),
			font_size: 16.,
			text_align: Align2::LEFT_TOP,
		}
	}
}

impl Div {
	pub fn new() -> Div {
		Self::default()
	}

	pub fn hover_fill(mut self, fill: impl Into<Background>) -> Div {
		self.hovered.fill = Some(fill.into());
		self
	}

	pub fn hover_scale(mut self, scale: f32) -> Div {
		self.hovered.scale = scale;
		self
	}

	pub fn hover_text<S: AsRef<str>>(mut self, text: S) -> Div {
		self.hovered.text = Some(text.as_ref().to_owned());
		self
	}

	fn draw(&self, ui: &mut Ui) -> (Rect, Response) {
		let (rect, response) = ui.allocate_exact_size(self.size, Sense::click());

		if let Some(shadow) = self.shadow {
			ui.painter().add(shadow.as_shape(rect, self.corner_radius));
		}

		let id = ui.make_persistent_id(format!("div-{}", self.id));
		let delta_time = ui.ctx().animate_bool(id, response.hovered());
		let scale = lerp(1.0..=self.hovered.scale, delta_time);
		let scaled_rect = Rect::from_center_size(rect.center(), self.size * scale);

		let (mesh, points) = rounded_gradient_mesh(
			scaled_rect,
			self.corner_radius,
			if response.hovered()
				&& let Some(fill) = &self.hovered.fill
			{
				fill
			} else {
				&self.fill
			},
		);
		ui.painter().add(Shape::mesh(mesh));

		if self.stroke != Stroke::NONE {
			ui.painter().add(Shape::closed_line(points, self.stroke));
		}

		if response.hovered() {
			ui.ctx().set_cursor_icon(self.cursor);
		}

		(rect, response)
	}

	pub fn add_to(self, ui: &mut Ui) -> Response {
		ui.add(self)
	}

	pub fn show<R>(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> Response {
		let (rect, response) = self.draw(ui);
		let builder = UiBuilder::new().max_rect(rect.shrink(self.inner_margin.leftf())).layout(Layout::top_down(Align::Center));
		let mut child_ui = ui.new_child(builder);
		add_contents(&mut child_ui);

		response
	}
}

impl Widget for Div {
	fn ui(self, ui: &mut Ui) -> Response {
		let (rect, response) = self.draw(ui);

		let id = ui.make_persistent_id(format!("div-{}", self.id));
		let delta_time = ui.ctx().animate_bool(id, response.hovered());
		let scale = lerp(1.0..=self.hovered.scale, delta_time);
		let scaled_rect = Rect::from_center_size(rect.center(), self.size * scale);

		let text = if response.hovered()
			&& let Some(text) = &self.hovered.text
		{
			Some(text)
		} else if let Some(text) = &self.text {
			Some(text)
		} else {
			None
		};

		if let Some(text) = text {
			let inner_rect = Rect::from_min_max(
				scaled_rect.min + vec2(self.inner_margin.leftf(), self.inner_margin.topf()),
				scaled_rect.max - vec2(self.inner_margin.rightf(), self.inner_margin.bottomf()),
			);

			let text_pos = pos2(
				match self.text_align.x() {
					Align::LEFT => inner_rect.left(),
					Align::Center => inner_rect.center().x,
					Align::RIGHT => inner_rect.right(),
				},
				match self.text_align.y() {
					Align::TOP => inner_rect.top(),
					Align::Center => inner_rect.center().y,
					Align::BOTTOM => inner_rect.bottom(),
				},
			);

			ui.painter()
				.text(text_pos, self.text_align, text, FontId::proportional(self.font_size * scale), self.text_color);
		}

		response
	}
}

fn rounded_gradient_mesh(rect: Rect, radius: f32, background: &Background) -> (Mesh, Vec<Pos2>) {
	let mut mesh = Mesh::default();
	let mut points = Vec::new();

	let radius = radius.min(rect.width() * 0.5).min(rect.height() * 0.5);

	let get_t = |x: f32, y: f32| -> f32 {
		match background {
			Background::Gradient(gradient) => {
				let u = ((x - rect.left()) / rect.width()).clamp(0.0, 1.0);
				let v = ((y - rect.top()) / rect.height()).clamp(0.0, 1.0);

				match gradient.direction {
					GradientDirection::ToLeft => 1.0 - u,
					GradientDirection::ToRight => u,
					GradientDirection::ToTop => 1.0 - v,
					GradientDirection::ToBottom => v,
					GradientDirection::ToTopLeft => 1.0 - (u + v) / 2.0,
					GradientDirection::ToTopRight => (u + 1.0 - v) / 2.0,
					GradientDirection::ToBottomLeft => (1.0 - u + v) / 2.0,
					GradientDirection::ToBottomRight => (u + v) / 2.0,
				}
			},
			Background::Solid(_) => 0.,
		}
	};

	if radius <= 0.0 {
		let mut add_corner = |pos: Pos2| {
			points.push(pos);
			match background {
				Background::Solid(color) => mesh.colored_vertex(pos, *color),
				Background::Gradient(gradient) => {
					let t = get_t(pos.x, pos.y);
					mesh.colored_vertex(pos, lerp_color(gradient.color1, gradient.color2, t));
				},
			}
		};

		add_corner(rect.left_top());
		add_corner(rect.right_top());
		add_corner(rect.right_bottom());
		add_corner(rect.left_bottom());

		mesh.add_triangle(0, 1, 2);
		mesh.add_triangle(0, 2, 3);

		return (mesh, points);
	}

	let center = rect.center();

	let center_index = mesh.vertices.len() as u32;

	match background {
		Background::Solid(color) => mesh.colored_vertex(center, *color),
		Background::Gradient(gradient) => mesh.colored_vertex(center, lerp_color(gradient.color1, gradient.color2, 0.5)),
	}

	let segments = 8;

	let corners = [
		(rect.left() + radius, rect.top() + radius, PI, PI + FRAC_PI_2),
		(rect.right() - radius, rect.top() + radius, PI + FRAC_PI_2, 2.0 * PI),
		(rect.right() - radius, rect.bottom() - radius, 0.0, FRAC_PI_2),
		(rect.left() + radius, rect.bottom() - radius, FRAC_PI_2, PI),
	];

	let mut ring = Vec::new();

	for (cx, cy, start, end) in corners {
		for i in 0..=segments {
			let t_angle = i as f32 / segments as f32;
			let a = start + (end - start) * t_angle;

			let x = cx + radius * a.cos();
			let y = cy + radius * a.sin();
			let pos = pos2(x, y);

			let t_color = get_t(x, y);
			let color = match background {
				Background::Solid(color) => *color,
				Background::Gradient(gradient) => lerp_color(gradient.color1, gradient.color2, t_color),
			};

			points.push(pos);
			ring.push(mesh.vertices.len() as u32);
			mesh.colored_vertex([x, y].into(), color);
		}
	}

	for i in 0..ring.len() {
		let a = ring[i];
		let b = ring[(i + 1) % ring.len()];
		mesh.add_triangle(center_index, a, b);
	}

	(mesh, points)
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
	let lerp = |x: u8, y: u8| -> u8 { (x as f32 + (y as f32 - x as f32) * t) as u8 };

	Color32::from_rgba_premultiplied(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()), lerp(a.a(), b.a()))
}

pub fn div() -> Div {
	Div::new()
}

pub fn button() -> Div {
	Div::new().cursor(CursorIcon::PointingHand).text_align(Align2::CENTER_CENTER).corner_radius(4.)
}

pub trait Listeners {
	fn on_click(&self, f: impl FnOnce());
}

impl Listeners for Response {
	fn on_click(&self, f: impl FnOnce()) {
		if self.clicked() {
			f();
		}
	}
}
