//! `cosmic-ext-snip`: select a region with the COSMIC screenshot portal, then
//! annotate it, copy it or save it.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use cosmic_ext_snip::app::{App, Flags};
use cosmic_ext_snip::{capture, clipboard};

#[derive(Parser, Debug)]
#[command(version, about = "Snip a region of the screen and annotate it")]
struct Args {
    /// Serve this PNG on the clipboard until something else is copied, then
    /// delete it. The editor starts this mode itself when you copy.
    #[arg(long, value_name = "PNG")]
    serve_clipboard: Option<PathBuf>,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args = Args::parse();

    if let Some(path) = args.serve_clipboard {
        let png = match std::fs::read(&path) {
            Ok(png) => png,
            Err(e) => {
                eprintln!("cosmic-ext-snip: cannot read {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        };
        let _ = std::fs::remove_file(&path);
        return match clipboard::serve(png) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("cosmic-ext-snip: {e}");
                ExitCode::FAILURE
            }
        };
    }

    // ashpd keeps one D-Bus connection for the whole process, and its reader
    // task runs on the runtime that opened it. A current-thread runtime only
    // runs tasks inside `block_on`, so once the editor opened, that task never
    // ran again and the save dialog's request hung. This runtime has its own
    // worker thread and lives until the program exits.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("cosmic-ext-snip: cannot start the async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    let snip = match runtime.block_on(capture::request()) {
        Ok(Some(snip)) => snip,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cosmic-ext-snip: {e}");
            return ExitCode::FAILURE;
        }
    };

    let width = (snip.width() as f32).clamp(560.0, 1600.0);
    let height = (snip.height() as f32 + 56.0).clamp(360.0, 1000.0);
    let settings = cosmic::app::Settings::default().size(cosmic::iced::Size::new(width, height));
    let result = cosmic::app::run::<App>(settings, Flags { snip });
    drop(runtime);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cosmic-ext-snip: {e}");
            ExitCode::FAILURE
        }
    }
}
