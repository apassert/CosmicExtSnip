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
    fn only_a_non_empty_image_png_is_read_back() {
        assert!(Png::try_from((vec![9], "image/png".to_string())).is_ok());
        assert!(Png::try_from((vec![], "image/png".to_string())).is_err());
        assert!(Png::try_from((vec![9], "text/plain".to_string())).is_err());
    }
}
