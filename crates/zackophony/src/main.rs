#![feature(mpmc_channel)]

use crate::ui::App;

mod audio;
mod keybindings;
mod soundboard;
mod theme;
mod ui;

pub const NAME: &str = "Zackophony";
pub const VERSION: &str = "0.1.0";

fn main() -> anyhow::Result<()> {
	App::start()?;

	Ok(())
}
