use std::{
	env, fs, os::unix::process::CommandExt, path::PathBuf, process::Command,
	rc::Rc, sync::LazyLock, thread, time::Duration,
};

use anyhow::{Result, anyhow};
use clap::Parser;
use compat::SteamCompat;
use serde::Deserialize;
use which::which;

pub mod compat;

pub const WALLPAPER_ENGINE_ID: u32 = 431960;

pub static STEAM_PATH: LazyLock<PathBuf> =
	LazyLock::new(|| dirs::home_dir().unwrap().join(".steam/steam"));
pub static STEAMAPPS: LazyLock<PathBuf> =
	LazyLock::new(|| STEAM_PATH.join("steamapps"));
pub static COMMON: LazyLock<PathBuf> =
	LazyLock::new(|| STEAMAPPS.join("common"));
pub static COMPATDATA_PATH: LazyLock<PathBuf> =
	LazyLock::new(|| STEAMAPPS.join("compatdata").join(431960.to_string()));
pub static WORKSHOP_CONTENT_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
	STEAMAPPS.join("workshop/content").join(431960.to_string())
});
pub static WALLPAPER_ENGINE_PATH: LazyLock<PathBuf> =
	LazyLock::new(|| COMMON.join("wallpaper_engine"));

static STEAM_BIN: LazyLock<PathBuf> =
	LazyLock::new(|| which("steam").unwrap());
static PGREP_BIN: LazyLock<PathBuf> =
	LazyLock::new(|| which("pgrep").unwrap());
static MAGICK_BIN: LazyLock<PathBuf> =
	LazyLock::new(|| which("magick").unwrap());
static CHAFA_BIN: LazyLock<PathBuf> =
	LazyLock::new(|| which("chafa").unwrap());
static XDOTOOL_BIN: LazyLock<PathBuf> =
	LazyLock::new(|| which("xdotool").unwrap());

#[derive(Parser)]
struct Args {
	/// Proton version folder name (e.g., "Proton 10.0" or "GE-Proton7-55") at
	/// ~/.steam/steam/compatibilitytools.d/ or
	/// ~/.steam/steam/steamapps/common/
	#[arg(short, long)]
	proton_version: String,
	/// Architecture: 64 or 32
	#[arg(short, long)]
	arch: String,
	/// Wallpaper IDs
	#[arg(short, long)]
	wallpaper_ids: Vec<String>,
}

#[derive(Deserialize)]
struct ProjectInfo {
	title: Option<String>,
	description: Option<String>,
}

fn wait_for_window(title: &str) {
	while !window_title_exists(&title) {
		thread::sleep(Duration::from_millis(100));
	}
}

/// Renders the first frame of the GIF or the JPG.
fn show_preview(dir: &PathBuf) -> Result<()> {
	let gif = dir.join("preview.gif");
	let jpg = dir.join("preview.jpg");

	if jpg.exists() {
		Command::new(CHAFA_BIN.as_path())
			.args([
				"--symbols=block",
				"--fill=block",
				"--size=40x20",
				jpg.to_str().unwrap(),
			])
			.status()?;
	} else if gif.exists() {
		const TMP: &'static str = "/tmp/chafa_preview.png";

		Command::new(MAGICK_BIN.as_path())
			.args([format!("{}[0]", gif.to_str().unwrap()), TMP.to_string()])
			.status()
			.expect("failed to extract first frame of gif");
		Command::new(CHAFA_BIN.as_path())
			.args(["--symbols=block", "--fill=block", "--size=40x20", TMP])
			.status()?;
		let _ = fs::remove_file(TMP);
	} else {
		println!("No preview image found in {:?}", dir);
	}

	Ok(())
}

fn show_info(dir: &PathBuf) {
	let json_path = dir.join("project.json");
	if let Ok(content) = fs::read_to_string(json_path) {
		if let Ok(info) = serde_json::from_str::<ProjectInfo>(&content) {
			if let Some(title) = info.title.clone() {
				println!("## {}", title);
			}
			if let Some(desc) = info.description {
				if info.title.is_some() {
					println!();
				}
				println!("{}", desc);
			}
		}
	}
}

fn window_class_exists(class: &str) -> bool {
	let status = Command::new(XDOTOOL_BIN.as_path())
		.args(["search", "--class", class])
		.status();
	if let Ok(s) = status {
		if s.success() {
			return true;
		}
	}
	false
}

fn window_title_exists(title: &str) -> bool {
	let status = Command::new(XDOTOOL_BIN.as_path())
		.args(["search", "--name", title])
		.status();
	if let Ok(s) = status {
		if s.success() {
			return true;
		}
	}
	false
}

fn we_is_running() -> bool {
	let output = Command::new(PGREP_BIN.as_path())
		.arg("-f")
		.arg("wallpaper32.exe")
		.output()
		.expect("failed to execute pgrep");

	if !output.stdout.is_empty() {
		true
	} else {
		let output = Command::new(PGREP_BIN.as_path())
			.arg("-f")
			.arg("wallpaper64.exe")
			.output()
			.expect("failed to execute pgrep");

		if !output.stdout.is_empty() {
			true
		} else {
			false
		}
	}
}

fn steam_is_running() -> bool {
	window_class_exists("steamwebhelper")
}

enum SteamOrProton {
	Steam,
	Proton(Rc<PathBuf>),
}

fn start_wallpaper(
	steam_or_proton: SteamOrProton,
	wallpaper_engine: &PathBuf,
	title: &String,
	file_path: &String,
) -> Result<()> {
	let args = [
		"-nobrowse",
		"-control",
		"openWallpaper",
		"-file",
		file_path,
		"-playInWindow",
		title,
		"-width",
		"1920",
		"-height",
		"1080",
	];

	match steam_or_proton {
		SteamOrProton::Steam => {
			Command::new(STEAM_BIN.as_path())
				.process_group(0)
				.arg("-applaunch")
				.arg("431960")
				.args(args)
				.spawn()
				.expect("failed to run proton Wallpaper Engine");
		}
		SteamOrProton::Proton(proton) => {
			Command::new(proton.as_path())
				.arg("run")
				.arg(&wallpaper_engine)
				.args(args)
				.spawn()
				.expect("failed to run proton Wallpaper Engine");
		}
	}

	Ok(())
}

fn main() -> Result<()> {
	let args = Args::parse();

	let sc = SteamCompat::from_name(&args.proton_version).ok_or(anyhow!(
		"Error: Proton folder not found: {:?}",
		args.proton_version
	))?;

	println!("{:#?}", sc);
	println!("{}", sc.internal_name());

	if args.arch != "64" && args.arch != "32" {
		eprintln!("Error: arch must be 64 or 32");
		std::process::exit(1);
	}
	let proton = Rc::new(sc.path.join("proton"));

	if args.wallpaper_ids.len() == 0 {
		eprintln!("Error: no wallpapers provided");
		return Ok(());
	}

	if !steam_is_running() {
		println!("Waiting for Steam to start...");
		println!("You must do this manually.");
	}
	while !steam_is_running() {
		thread::sleep(Duration::from_millis(100));
	}

	// let mut _steam_command = None;
	// if !steam_is_running() {
	// 	_steam_command =
	// 		Some(Command::new(STEAM_BIN.as_path()).process_group(0).spawn()?);
	// 	// eprintln!("Error: Steam isn't running");
	// 	// return Ok(());
	// }

	let wallpaper_engine =
		WALLPAPER_ENGINE_PATH.join(format!("wallpaper{}.exe", args.arch));
	if !wallpaper_engine.exists() {
		eprintln!("Wallpaper Engine not found: {:?}", wallpaper_engine);
		std::process::exit(1);
	}

	Command::new(STEAM_BIN.as_path())
		.arg("+app_stop")
		.arg(WALLPAPER_ENGINE_ID.to_string())
		.status()?;

	while we_is_running() {
		Command::new(STEAM_BIN.as_path())
			.arg("+app_stop")
			.arg(WALLPAPER_ENGINE_ID.to_string())
			.status()?;
		thread::sleep(Duration::from_millis(100));
	}

	sc.apply_to_game(WALLPAPER_ENGINE_ID)?;

	// Set the env variables Proton needs.
	unsafe {
		env::set_var("PROTON_DIR", &sc.path.as_path());
		env::set_var(
			"STEAM_COMPAT_DATA_PATH",
			COMPATDATA_PATH.to_string_lossy().to_string(),
		);
		env::set_var(
			"STEAM_COMPAT_CLIENT_INSTALL_PATH",
			STEAM_PATH.to_string_lossy().to_string(),
		);
	}

	for (i, wallpaper_id) in args.wallpaper_ids.iter().enumerate() {
		let title = format!("Wallpaper #{}", i);
		let dir = WORKSHOP_CONTENT_PATH.join(wallpaper_id);
		// Proton pretends that the Z: drive on "Windows" is the root folder.
		let file_path =
			format!("Z:{}", dir.join("project.json").to_str().unwrap());

		println!("\n# {}", title);
		show_info(&dir);
		show_preview(&dir)?;

		start_wallpaper(
			if !we_is_running() {
				SteamOrProton::Steam
			} else {
				SteamOrProton::Proton(proton.clone())
			},
			&wallpaper_engine,
			&title,
			&file_path,
		)?;

		wait_for_window(&title);
	}

	// Stop it from rendering stuff in the background.
	Command::new(proton.as_path())
		.arg("run")
		.arg(&wallpaper_engine)
		.args(["-nobrowse", "-control", "stop"])
		.spawn()
		.expect("failed to run proton Wallpaper Engine");

	Ok(())
}
