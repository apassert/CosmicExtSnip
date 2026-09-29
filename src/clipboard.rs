//! On Wayland the process that copies owns the clipboard, and the content
//! vanishes when it exits. So the process that copied keeps serving after its
//! window has closed: the editor hands the PNG over and exits its window, and
//! `main` serves it until another client takes the clipboard.
//!
//! It used to re-execute itself as a detached helper. Inside a Flatpak sandbox
//! that helper dies with the sandbox, which ends when its main process does,
//! and the copy would be lost the moment the window closed.

use std::sync::{Arc, Mutex};

use wl_clipboard_rs::copy::{MimeType, Options, Source};

/// Where the editor leaves the PNG it copied, for `main` to serve.
pub type Handoff = Arc<Mutex<Option<Vec<u8>>>>;

pub fn handoff() -> Handoff {
    Arc::new(Mutex::new(None))
}

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
