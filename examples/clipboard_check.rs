//! Checks that the clipboard holds exactly what the editor copies for an
//! unannotated snip of the given image. With no image it only prints what the
//! clipboard offers as `image/png`.
//!
//! ```text
//! cargo run --release --example clipboard_check -- some.png
//! ```
//!
//! `scripts/copy-check.sh` runs it after the editor has copied and gone.
use std::io::Read;
use std::process::ExitCode;

use cosmic_ext_snip::render;
use wl_clipboard_rs::paste::{ClipboardType, MimeType, Seat, get_contents};

fn pasted_png() -> Result<Vec<u8>, String> {
    let (mut pipe, _mime) = get_contents(
        ClipboardType::Regular,
        Seat::Unspecified,
        MimeType::Specific("image/png"),
    )
    .map_err(|e| format!("the clipboard offers no image/png: {e}"))?;
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read the clipboard: {e}"))?;
    Ok(bytes)
}

fn check(source: Option<String>) -> Result<String, String> {
    let pasted = pasted_png()?;
    let Some(source) = source else {
        return Ok(format!(
            "the clipboard holds {} bytes of image/png",
            pasted.len()
        ));
    };
    let bytes = std::fs::read(&source).map_err(|e| format!("cannot read {source}: {e}"))?;
    let snip = render::decode_png(&bytes)?;
    let expected = render::encode_png(&render::composite(&snip, []))?;
    if pasted != expected {
        return Err(format!(
            "the clipboard holds {} bytes of image/png, not the {} the editor copies for {source}",
            pasted.len(),
            expected.len()
        ));
    }
    Ok(format!(
        "the clipboard holds the copy of {source}: {} bytes, {}x{}",
        pasted.len(),
        snip.width(),
        snip.height()
    ))
}

fn main() -> ExitCode {
    match check(std::env::args().nth(1)) {
        Ok(found) => {
            println!("clipboard-check: {found}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("clipboard-check: {e}");
            ExitCode::FAILURE
        }
    }
}
