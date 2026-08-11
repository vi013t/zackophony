use std::{
	env,
	path::{Path, PathBuf},
};

use mslnk::ShellLink;
use winreg::{RegKey, enums::HKEY_CURRENT_USER};

static EMBEDDED_APP: &[u8] = include_bytes!("../../../target/release/zackophony.exe");

fn main() -> anyhow::Result<()> {
	// install_vbcable()?;
	install_zackophony()?;
	create_start_menu_shortcut()?;
	register_startup()?;

	Ok(())
}

fn install_zackophony() -> anyhow::Result<()> {
	let install_location = PathBuf::from(env::var("ProgramFiles")?).join("Zackophony");
	std::fs::create_dir_all(&install_location)?;
	std::fs::write(install_location.join("Zackophony.exe"), EMBEDDED_APP)?;

	Ok(())
}

fn create_start_menu_shortcut() -> anyhow::Result<()> {
	let shortcut_path = Path::new(&std::env::var("ProgramData")?)
		.join("Microsoft")
		.join("Windows")
		.join("Start Menu")
		.join("Programs")
		.join("Zackophony")
		.join("Zackophony.lnk");

	let target = Path::new(&env::var("ProgramFiles")?).join("Zackophony").join("Zackophony.exe");

	std::fs::create_dir_all(shortcut_path.parent().unwrap())?;

	let link = ShellLink::new(&target)?;
	link.create_lnk(shortcut_path)?;

	Ok(())
}

fn install_vbcable() -> std::io::Result<()> {
	runas::Command::new(
		std::env::current_exe()?
			.parent()
			.unwrap()
			.join("Resources")
			.join("Virtual Cable Driver")
			.join("VBCABLE_Setup_x64.exe"),
	)
	.status()?;

	Ok(())
}

fn register_startup() -> anyhow::Result<()> {
	let install_location = PathBuf::from(env::var("ProgramFiles")?).join("Zackophony");
	let exe = install_location.join("Zackophony.exe");

	let hkcu = RegKey::predef(HKEY_CURRENT_USER);
	let (run, _) = hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")?;

	run.set_value("Zackophony", &format!("\"{}\"", exe.display()))?;

	Ok(())
}
