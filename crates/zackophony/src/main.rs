#![feature(mpmc_channel)]

use crate::ui::App;

mod audio;
mod soundboard;
mod theme;
mod ui;

pub const NAME: &str = "Zackophony";

fn main() -> anyhow::Result<()> {
	App::start()?;

	Ok(())
}
