//! Whether this process runs inside a Flatpak sandbox.
//!
//! It matters for the clipboard. COSMIC hides the data-control protocols
//! (`ext_data_control_manager_v1`, `zwlr_data_control_manager_v1`) from
//! sandboxed clients - measured on 2026-09-29: 58 globals on the host, 35 in the
//! sandbox, both data-control managers among the missing. Without them nothing
//! can be read from or served to the clipboard except through a window of our
//! own, so a sandboxed snip copies through its window and keeps the process
//! running afterwards.

use std::path::Path;

pub fn sandboxed() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists()
}
