#![feature(mpmc_channel)]

use crate::ui::App;

mod audio;
mod soundboard;
mod ui;

pub const NAME: &str = "Soundboard";

fn main() -> anyhow::Result<()> {
    App::start()?;

    Ok(())
}
