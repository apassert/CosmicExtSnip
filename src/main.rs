//! `cosmic-ext-snip`: select a region with the COSMIC screenshot portal, then
//! annotate it, copy or save it.

use std::process::ExitCode;

use clap::Parser;
use cosmic_ext_snip::app::{App, Flags};
use cosmic_ext_snip::clipboard;

#[derive(Parser, Debug)]
#[command(version, about = "Snip a region of the screen and annotate it")]
struct Args {
    /// Annotate this PNG instead of taking a snip.
    file: Option<std::path::PathBuf>,
    /// Internal: serve the PNG on stdin as the clipboard (clipboard.rs).
    #[arg(long, hide = true)]
    serve_clipboard: bool,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args = Args::parse();
    if args.serve_clipboard {
        return finish(clipboard::serve_stdin());
    }

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
            open: args.file,
        },
    );
    if let Err(e) = result {
        eprintln!("cosmic-ext-snip: {e}");
        return ExitCode::FAILURE;
    }
    // Only when the editor could not re-execute itself to serve its copy: it
    // is served here, after the app has ended. See clipboard.rs.
    let copied = handoff.lock().ok().and_then(|mut slot| slot.take());
    finish(copied.map(clipboard::serve).unwrap_or(Ok(())))
}

fn finish(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cosmic-ext-snip: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_argument_the_editor_re_executes_with_is_the_one_parsed_here() {
        let args = Args::try_parse_from(["cosmic-ext-snip", clipboard::SERVE_ARG]).expect("parses");
        assert!(args.serve_clipboard);
        assert!(args.file.is_none());
    }
}
