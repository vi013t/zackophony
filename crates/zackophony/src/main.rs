#![feature(mpmc_channel)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use crate::ui::App;

mod audio;
mod keybindings;
mod soundboard;
mod ui;

pub const NAME: &str = "Zackophony";
pub const VERSION: &str = "0.1.0";

fn main() -> anyhow::Result<()> {
	App::start()?;

	Ok(())
}
