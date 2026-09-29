//! On Wayland the process that copies owns the clipboard, and the content
//! vanishes when it exits. The editor re-executes itself with
//! `--serve-clipboard <png>`, detached, to keep serving after the window closes.

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use wl_clipboard_rs::copy::{MimeType, Options, Source};

/// Serves `png` as `image/png` until another client takes the clipboard.
pub fn serve(png: Vec<u8>) -> Result<(), String> {
    let mut options = Options::new();
    options.foreground(true);
    options
        .copy(
            Source::Bytes(png.into_boxed_slice()),
            MimeType::Specific("image/png".into()),
        )
        .map_err(|e| format!("cannot set the clipboard: {e}"))
}

/// Writes `png` to a temporary file and starts a detached server for it.
pub fn spawn_server(png: &[u8]) -> Result<(), String> {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("cosmicsnip-clip-{}.png", std::process::id()));
    std::fs::write(&path, png).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    spawn_for(&path)
}

fn spawn_for(path: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find own executable: {e}"))?;
    Command::new(exe)
        .arg("--serve-clipboard")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot start the clipboard server: {e}"))
}
