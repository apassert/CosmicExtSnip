//! On Wayland the process that copies owns the clipboard, and the content
//! vanishes when it exits. So the process that copied keeps serving after its
//! window has closed, until another client takes the clipboard.
//!
//! Outside a sandbox it does so as a fresh image of itself: the editor puts
//! the PNG in an anonymous in-memory file and re-executes this binary with
//! `--serve-clipboard`, reading that file on stdin (`serve_in_new_image`). The
//! process must not simply go on after the app has ended: iced's Wayland
//! thread borrows winit's display, outlives it, and frees its objects on the
//! closed connection seconds later. Until libwayland 1.26 that went unnoticed;
//! with it the process is killed (SIGSEGV in `wl_map_insert_at`) and the copy
//! with it. `exec` ends every other thread, and it keeps the process ID, which
//! matters where the desktop started the app as a service: a detached child
//! would be stopped with the service the moment the main process exits.
//!
//! Should the re-execution fail, the PNG is left in a hand-off slot and `main`
//! serves it in this process after the app has ended, as before.
//!
//! Inside a Flatpak sandbox none of this applies: the copy lives in the
//! window's own clipboard and the app keeps running (see sandbox.rs).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use wl_clipboard_rs::copy::{MimeType, Options, Source};

/// The argument that makes this binary serve the PNG on its stdin.
pub const SERVE_ARG: &str = "--serve-clipboard";

/// Where the editor leaves the PNG it copied for `main` to serve, when it
/// could not hand it to a fresh image of the process.
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

/// The image this process runs, whatever has happened to its file since.
const RUNNING_IMAGE: &str = "/proc/self/exe";

/// Replaces this process with one that serves `png` and nothing else. Returns
/// only if that failed; the caller then still has the PNG and the editor.
pub fn serve_in_new_image(png: &[u8]) -> io::Error {
    let copy = match in_memory_file(png) {
        Ok(file) => file,
        Err(e) => return e,
    };
    // By its own path, so the process keeps its name. That path is gone when
    // the binary was replaced while the editor was open (an update): then the
    // image this process runs is executed again instead.
    let own = std::env::current_exe().unwrap_or_else(|_| PathBuf::from(RUNNING_IMAGE));
    let failed = exec_serving(&own, &copy);
    if own == Path::new(RUNNING_IMAGE) {
        return failed;
    }
    exec_serving(Path::new(RUNNING_IMAGE), &copy)
}

fn exec_serving(exe: &Path, copy: &File) -> io::Error {
    let stdin = match copy.try_clone() {
        Ok(file) => file,
        Err(e) => return e,
    };
    Command::new(exe)
        .arg(SERVE_ARG)
        .stdin(Stdio::from(stdin))
        .exec()
}

/// What `--serve-clipboard` does: serves the PNG on stdin until another client
/// takes the clipboard.
pub fn serve_stdin() -> Result<(), String> {
    let mut png = Vec::new();
    io::stdin()
        .read_to_end(&mut png)
        .map_err(|e| format!("cannot read the copy from stdin: {e}"))?;
    if png.is_empty() {
        return Err("no copy on stdin to serve".into());
    }
    serve(png)
}

/// `bytes` in a file that exists in memory only, positioned at its start: no
/// name, nothing on disk, gone with the last process that holds it.
fn in_memory_file(bytes: &[u8]) -> io::Result<File> {
    let fd = rustix::fs::memfd_create("cosmic-ext-snip-copy", rustix::fs::MemfdFlags::CLOEXEC)?;
    let mut file = File::from(fd);
    file.write_all(bytes)?;
    file.seek(SeekFrom::Start(0))?;
    Ok(file)
}

/// A PNG for the window-based clipboard, the only one a sandboxed app has.
#[derive(Clone, Debug)]
pub struct Png(pub Vec<u8>);

const PNG: &str = "image/png";

impl cosmic::iced::clipboard::mime::AsMimeTypes for Png {
    fn available(&self) -> std::borrow::Cow<'static, [String]> {
        std::borrow::Cow::Owned(vec![PNG.to_string()])
    }

    fn as_bytes(&self, mime_type: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
        (mime_type == PNG).then(|| std::borrow::Cow::Owned(self.0.clone()))
    }
}

impl cosmic::iced::clipboard::mime::AllowedMimeTypes for Png {
    fn allowed() -> std::borrow::Cow<'static, [String]> {
        std::borrow::Cow::Owned(vec![PNG.to_string()])
    }
}

impl TryFrom<(Vec<u8>, String)> for Png {
    type Error = ();

    fn try_from((bytes, mime): (Vec<u8>, String)) -> Result<Self, ()> {
        (mime == PNG && !bytes.is_empty())
            .then_some(Png(bytes))
            .ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::iced::clipboard::mime::{AllowedMimeTypes, AsMimeTypes};

    #[test]
    fn a_png_is_offered_as_image_png_and_nothing_else() {
        let png = Png(vec![1, 2, 3]);
        assert_eq!(png.available().as_ref(), ["image/png".to_string()]);
        assert_eq!(png.as_bytes("image/png").as_deref(), Some(&[1u8, 2, 3][..]));
        assert!(png.as_bytes("text/plain").is_none());
        assert_eq!(Png::allowed().as_ref(), ["image/png".to_string()]);
    }

    #[test]
    fn the_handed_over_copy_is_read_back_whole_from_its_start() {
        let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
        let mut file = in_memory_file(&png).expect("an in-memory file");
        let mut read = Vec::new();
        file.read_to_end(&mut read).expect("readable");
        assert_eq!(read, png);
    }

    #[test]
    fn only_a_non_empty_image_png_is_read_back() {
        assert!(Png::try_from((vec![9], "image/png".to_string())).is_ok());
        assert!(Png::try_from((vec![], "image/png".to_string())).is_err());
        assert!(Png::try_from((vec![9], "text/plain".to_string())).is_err());
    }
}
