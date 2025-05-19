use std::{
	path::PathBuf,
	process::{Command, ExitStatus},
	sync::LazyLock,
};

use anyhow::{Result, anyhow};
use heck::ToSnakeCase;
use pomsky::options::CompileOptions;
use regex::Regex;

use crate::{COMMON, STEAM_BIN, STEAM_PATH};

pub static COMPATIBILITYTOOLS_D: LazyLock<PathBuf> =
	LazyLock::new(|| STEAM_PATH.join("compatibilitytools.d"));

#[derive(Debug)]
pub struct SteamCompat {
	pub name: String,
	pub path: PathBuf,
	pub builtin: bool,
}
impl SteamCompat {
	pub fn internal_name(&self) -> String {
		if self.builtin {
			// Steam has internal names for Proton versions.
			// Always prefixed with `proton_`.
			// If the version is a word it's `word`.
			// If the version is a number it's horrible.
			// `Proton 10.0` -> `proton_10`
			// `Proton 9.0 (Beta) -> `proton_9`
			// `Proton 4.11` -> `proton_411`
			// This may break for Steam eventually...
			let snake = self.name.to_snake_case();

			let re = Regex::new(
				&pomsky::Expr::parse_and_compile(
					include_str!("./internal.pom"),
					CompileOptions::default(),
				)
				.0
				.unwrap(),
			)
			.unwrap();

			if let Some(caps) = re.captures(&snake) {
				if let Some(matched) = caps.name("name") {
					return matched
						.as_str()
						.to_string()
						// Diabolical way to remove extra underscores.
						.replace("proton_", "proton-")
						.replace("_", "")
						.replace("proton-", "proton_");
				}
			}

			eprintln!(
				"Getting to this code branch is very bad... Continuing anyway!!"
			);

			snake
		} else {
			self.name.clone()
		}
	}

	pub fn from_name(name: &String) -> Option<SteamCompat> {
		let common_dir = COMMON.join(name);
		let d_dir = COMPATIBILITYTOOLS_D.join(name);

		match (common_dir.exists(), d_dir.exists()) {
			(true, true) => Some(SteamCompat {
				name: name.clone(),
				path: d_dir,
				builtin: false,
			}),
			(true, false) => Some(SteamCompat {
				name: name.clone(),
				path: common_dir,
				builtin: true,
			}),
			(false, true) => Some(SteamCompat {
				name: name.clone(),
				path: d_dir,
				builtin: false,
			}),
			(false, false) => None,
		}
	}

	/// https://developer.valvesoftware.com/wiki/Command_line_options#Command-Line_Parameters
	/// https://gist.github.com/davispuh/6600880
	pub fn apply_to_game(&self, id: u32) -> Result<ExitStatus> {
		Command::new(STEAM_BIN.as_path())
			.arg("+app_change_compat_tool")
			.arg(id.to_string())
			.arg(self.internal_name())
			.status()
			.map_err(|e| anyhow!("{}", e))
	}
}
