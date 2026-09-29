# Changelog

All notable changes to CosmicSnip are documented in this file.

## [2.0.0] - 2026-09-25

Rewritten in Rust on libcosmic, following cosmic-screenshot's architecture,
with CosmicSnip's annotation editor after the snip.

### Fixed
- **A snip can span every monitor.** Selection is drawn by the COSMIC
  screenshot portal (`xdg-desktop-portal-cosmic`), the way `cosmic-screenshot
  --interactive` does it, instead of by CosmicSnip's own GTK layer-shell
  overlays, which covered one screen when `gtk4-layer-shell` was missing.
- **Esc exits without crashing.** The editor is an ordinary toplevel window.
  The old editor closed while layer-shell overlays were still alive, which the
  overlay code itself documents as crashing on COSMIC.
- **A copy survives the editor closing.** On Wayland the copying process owns
  the clipboard; `cosmicsnip --serve-clipboard` keeps serving the PNG after
  the window closes and exits when something else is copied.

### Changed
- The editor remembers the colour and the pen and highlighter widths in
  `~/.config/cosmicsnip/editor.conf`; every snip starts with the pen.
- One-shot: run, select, annotate, copy or save, exit. Bind it to a shortcut.
- Installed with `just install` (to `~/.local`) or
  `sudo just prefix=/usr install`.

### Removed
- The Python/GTK code, tray icon, autostart entry, single-instance daemon,
  Debian packaging and the Python audit tooling. They remain in history.
- Drawing beyond the snip's edge and trimming the export to the drawn bounds.

## [1.0.4] - 2026-03-22

### Fixed
- Overlay windows now persist and are reused via `reconfigure()` — fixes ghost surface accumulation that caused stacked panel bars and duplicated dock icons on COSMIC Wayland.
- Removed `DBusActivatable` from desktop entry and source installer — fixes session crash on COSMIC compositor.
- Synced all release metadata dates and descriptions across CHANGELOG, metainfo.xml, and debian/changelog.
- Removed unused `Callable` import from overlay module.

### Added
- Maintainability audit script (`scripts/audit-maintainability.sh`) with policy drift, complexity, duplication, and dead-code checks.
- CI maintainability job in GitHub Actions workflow with artifact upload.
- Release audit log (`RELEASE_AUDIT_LOG.md`) for SOC 2 Type II compliance traceability.

### Security
- Full security audit completed: bandit clean, no risky APIs, path/symlink hardening verified.
- CISO risk acceptance recorded for all open items with review checkpoint 2026-06-30.

## [1.0.2] - 2026-03-21

### Changed
- Distro-submission polish: professional docstrings, documented constants, cleaned redundant comments.
- Removed Pillow dependency — all image handling now uses native GdkPixbuf (smaller attack surface).
- AppStream metadata: added OARS content rating, screenshots section, developer contact.
- Desktop entry: added `TryExec` for freedesktop.org compliance.
- Added `CONTRIBUTING.md` with development setup, commit conventions, and PR workflow.
- Build and install scripts hardened with file existence checks and idempotent PATH setup.
- Security CI workflow improved: pinned audit deps, bandit scan clean.

## [1.0.1] - 2026-03-21

### Fixed
- Reused overlay windows across captures (including topology changes) to reduce hidden window accumulation risk.
- Enabled per-monitor overlay flow for single-monitor setups (instead of forced scaled fallback).
- Hardened security path checks:
  - save-path blocking now uses real path ancestry checks (not string prefix checks),
  - XDG path validation now uses ancestry checks,
  - temp cleanup validates path is inside temp root and rejects symlink targets.
- Added repository security policy and disclosure guidance (`SECURITY.md`).

## [1.0.0] - 2026-03-16

### Added
- First stable release.
- Libadwaita editor UI (HeaderBar, ToolbarView, ToastOverlay).
- System tray icon via DBus StatusNotifierItem.
- Persistent app lifecycle between snips.
- Out-of-bounds annotation with auto-trim transparent PNG output.
- Multi-monitor layer-shell overlays with shared selection state.
- Monitor detection with config caching.

## [0.2.0] - 2026-03-16

### Added
- Multi-monitor support via per-monitor overlays.
- Security hardening:
  - TOCTOU-safe temp file chmod path,
  - decompression bomb limits,
  - XDG path sanitization,
  - save path validation,
  - log/temp permission hardening.

## [0.1.0] - 2026-03-13

### Added
- Initial release.
- Screen capture via `cosmic-screenshot`.
- Drag-to-select overlay.
- Annotation editor with pen/highlighter/arrow/rectangle.
- GTK4 native clipboard support.
