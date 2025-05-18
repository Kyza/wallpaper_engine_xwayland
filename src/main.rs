use std::{
	env, fs, path::PathBuf, process::Command, rc::Rc, sync::LazyLock, thread,
	time::Duration,
};

use anyhow::Result;
use clap::Parser;
use serde::Deserialize;
use which::which;

pub static STEAM_PATH: LazyLock<PathBuf> =
	LazyLock::new(|| dirs::home_dir().unwrap().join(".steam/steam"));
pub static STEAMAPPS: LazyLock<PathBuf> =
	LazyLock::new(|| STEAM_PATH.join("steamapps"));
pub static COMMON: LazyLock<PathBuf> =
	LazyLock::new(|| STEAMAPPS.join("common"));
pub static COMPATDATA_PATH: LazyLock<PathBuf> =
	LazyLock::new(|| STEAMAPPS.join("compatdata/431960"));
pub static WORKSHOP_CONTENT_PATH: LazyLock<PathBuf> =
	LazyLock::new(|| STEAMAPPS.join("workshop/content/431960"));
pub static WALLPAPER_ENGINE_PATH: LazyLock<PathBuf> =
	LazyLock::new(|| COMMON.join("wallpaper_engine"));

#[derive(Parser)]
struct Args {
	/// Proton version folder name (e.g., "Proton 10.0" or "GE-Proton7-55")
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

fn wait_for_window(title: &str) -> Result<()> {
	let xdotool = Rc::new(which("xdotool")?);
	let start = std::time::Instant::now();
	while start.elapsed() < Duration::from_secs(20) {
		let status = Command::new(xdotool.as_path())
			.args(["search", "--name", title])
			.status();
		if let Ok(s) = status {
			if s.success() {
				return Ok(());
			}
		}
		thread::sleep(Duration::from_millis(100));
	}
	Err(anyhow::anyhow!("Timed out waiting for window: {}", title))
}

/// Renders the first frame of the GIF or the JPG.
fn show_preview(dir: &PathBuf) -> Result<()> {
	let gif = dir.join("preview.gif");
	let jpg = dir.join("preview.jpg");

	let chafa = which("chafa")?;
	if jpg.exists() {
		Command::new(chafa)
			.args([
				"--symbols=block",
				"--fill=block",
				"--size=40x20",
				jpg.to_str().unwrap(),
			])
			.status()?;
	} else if gif.exists() {
		let magick = which("magick")?;

		const TMP: &'static str = "/tmp/chafa_preview.png";

		Command::new(magick)
			.args([format!("{}[0]", gif.to_str().unwrap()), TMP.to_string()])
			.status()
			.expect("failed to extract first frame of gif");
		Command::new(chafa)
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

fn main() -> Result<()> {
	let args = Args::parse();

	if args.arch != "64" && args.arch != "32" {
		eprintln!("Error: arch must be 64 or 32");
		std::process::exit(1);
	}

	let proton_dir = COMMON.join(&args.proton_version);
	if !proton_dir.exists() {
		eprintln!("Error: Proton folder not found: {:?}", proton_dir);
		std::process::exit(1);
	}

	if args.wallpaper_ids.len() == 0 {
		eprintln!("Error: no wallpapers provided");
	}

	let wallpaper_engine =
		WALLPAPER_ENGINE_PATH.join(format!("wallpaper{}.exe", args.arch));
	if !wallpaper_engine.exists() {
		eprintln!("Wallpaper Engine not found: {:?}", wallpaper_engine);
		std::process::exit(1);
	}

	// Set the env variables Proton needs.
	unsafe {
		env::set_var("PROTON_DIR", &proton_dir);
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
			format!("Z:{}", dir.join("scene.pkg").to_str().unwrap());

		println!("\n# {}", title);
		show_info(&dir);
		show_preview(&dir)?;

		Command::new(proton_dir.join("proton"))
			.arg("run")
			.arg(&wallpaper_engine)
			.args([
				"-nobrowse",
				"-control",
				"openWallpaper",
				"-file",
				&file_path,
				"-playInWindow",
				&title,
				"-width",
				"1920",
				"-height",
				"1080",
			])
			.spawn()
			.expect("failed to run proton Wallpaper Engine");

		wait_for_window(&title)?;
	}

	// Stop it from rendering stuff in the background.
	Command::new(proton_dir.join("proton"))
		.arg("run")
		.arg(&wallpaper_engine)
		.args(["-nobrowse", "-control", "stop"])
		.spawn()
		.expect("failed to run proton Wallpaper Engine");

	Ok(())
}
