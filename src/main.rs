//! `cosmic-ext-snip`: select a region with the COSMIC screenshot portal, then
//! annotate it, copy or save it.

use std::process::ExitCode;

use clap::Parser;
use cosmic_ext_snip::app::{App, Flags};
use cosmic_ext_snip::clipboard;

#[derive(Parser, Debug)]
#[command(version, about = "Snip a region of the screen and annotate it")]
struct Args {}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let _args = Args::parse();

    // No window until the snip is taken (app.rs), and one process: launched
    // again, it asks the running one for a new snip and exits. The process does
    // not end with its window by itself - the app decides (app.rs,
    // close_window), because a sandboxed copy lives in this process.
    let settings = cosmic::app::Settings::default()
        .no_main_window(true)
        .exit_on_close(false);
    let handoff = clipboard::handoff();
    let result = cosmic::app::run_single_instance::<App>(
        settings,
        Flags {
            handoff: handoff.clone(),
        },
    );
    if let Err(e) = result {
        eprintln!("cosmic-ext-snip: {e}");
        return ExitCode::FAILURE;
    }
    // Outside a sandbox a copy is served by this process after the app has
    // ended, until something else is copied: see clipboard.rs.
    let copied = handoff.lock().ok().and_then(|mut slot| slot.take());
    match copied.map(clipboard::serve).unwrap_or(Ok(())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cosmic-ext-snip: {e}");
            ExitCode::FAILURE
        }
    }
}
