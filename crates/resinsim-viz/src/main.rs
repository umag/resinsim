//! Thin entry point for the resinsim-viz binary.
//!
//! Parses CLI args, validates `--screenshot` (pre-LogPlugin, uses
//! `eprintln`), delegates to [`resinsim_viz::run`], and translates
//! `AppExit::Error` to `process::exit()`.

use bevy::prelude::AppExit;
use clap::Parser;

fn main() {
    let mut args = resinsim_viz::Args::parse();

    if let Some(input) = args.screenshot.as_deref() {
        match resinsim_viz::screenshot::validate_screenshot_path(input) {
            Ok(resolved) => {
                args.screenshot = Some(resolved);
            }
            Err(err) => {
                eprintln!(
                    "{}",
                    resinsim_viz::screenshot::format_path_error(input, &err)
                );
                std::process::exit(resinsim_viz::EXIT_SCREENSHOT_BAD_PATH as i32);
            }
        }
    }

    let exit = resinsim_viz::run(args);
    if let AppExit::Error(code) = exit {
        std::process::exit(code.get() as i32);
    }
}
