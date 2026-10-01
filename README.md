# Snip for COSMIC™

Snip a region of the screen, annotate it, and copy or save it - on the COSMIC™
desktop. `cosmic-ext-snip` is a third-party application, not official COSMIC
software.

The selection is drawn by the desktop's own **XDG Desktop Portal**, exactly as
`cosmic-screenshot --interactive` does, so a snip can span every monitor. The
annotation editor then opens on the result. It is a Rust application on
[libcosmic](https://github.com/pop-os/libcosmic), so it looks like the rest of
the desktop.

Based on [CosmicSnip](https://github.com/taran3030/cosmicsnip) by itssoup,
rewritten in Rust for 2.0.

- Version history: [`CHANGELOG.md`](CHANGELOG.md)
- Security policy: [`SECURITY.md`](SECURITY.md)
- Contributing: [`CONTRIBUTING.md`](CONTRIBUTING.md)
- Architecture: [`ARCHITECTURE.md`](ARCHITECTURE.md)

## AI disclosure

Version 2.0 was written with an AI coding agent (Anthropic's Claude), directed,
reviewed and tested on a COSMIC desktop by the maintainer. That covers
essentially all of it: the Rust sources under `src/`, `Cargo.toml`, the
`justfile`, the desktop entry and metainfo under `data/`, and this and the
other documents. The icon is the original CosmicSnip artwork. Releases before
2.0 are the original Python CosmicSnip and are not covered by this note.

---

## Features

**Capture** (drawn by the COSMIC screenshot portal)
- Drag a region on any monitor, or across several
- `Enter` takes it, `Esc` cancels and the app exits

**Annotate**
- Pen, highlighter, arrow, rectangle, circle (ellipse) and text
- Text: click where it goes and type; it is drawn in the selected colour and
  in COSMIC's interface font. `Enter` or a click elsewhere keeps it, `Esc`
  drops it, `+` / `-` set the size of the next text
- Hold the pointer still for a second while drawing with the pen or
  highlighter and the stroke becomes a straight line from where it started;
  it then follows the pointer until you let go - as in the Windows Snipping Tool
- A **New snip** button (and `Ctrl+N`) to select another region
- `cosmic-ext-snip image.png` annotates an existing image instead of taking a snip
- 6-colour palette and adjustable stroke width
- Undo (`Ctrl+Z`, up to 200 steps)
- Remembers your colour, stroke widths and text size for the next snip; every snip starts with the pen

**Output**
- `Ctrl+C` copies the annotated snip at full resolution and closes; it stays
  on the clipboard after the window has closed
- `Ctrl+S` saves a PNG, by default under `~/Pictures/Screenshots/`

### Keyboard shortcuts (editor)

| Key | Action |
|-----|--------|
| `P` `H` `A` `R` `C` `T` | Pen / Highlighter / Arrow / Rectangle / Circle / Text |
| `+` / `-` | Thicker / thinner stroke, larger / smaller text |
| `Ctrl+C` | Copy to the clipboard and close |
| `Ctrl+S` | Save as PNG and close |
| `Ctrl+Z` | Undo |
| `Ctrl+N` | New snip |
| `Esc`, `Ctrl+Q` | Close |

---

## Install

Requires Rust (1.85 or newer), [`just`](https://github.com/casey/just), and
the libraries libcosmic links against:

```bash
sudo apt install pkg-config libxkbcommon-dev libwayland-dev libfontconfig-dev libfreetype-dev
git clone https://github.com/apassert/CosmicExtSnip.git
cd CosmicExtSnip
just install                 # into ~/.local
# or: sudo just prefix=/usr install
```

The first build compiles libcosmic and takes several minutes.

### Set up a keyboard shortcut

**COSMIC Settings → Keyboard → Custom Shortcuts → +**

| Field | Value |
|-------|-------|
| Name | Snip |
| Command | `cosmic-ext-snip` |
| Shortcut | `Super+Shift+S` |

---

## Uninstall

```bash
just uninstall               # or: sudo just prefix=/usr uninstall
```

---

## Develop

```bash
just check                   # cargo test + clippy -D warnings
just run
```

---

## Security

Snip for COSMIC is designed to handle screenshots safely. Screenshots are sensitive data — they can contain passwords, tokens, personal information.

Security policy and coordinated disclosure: [`SECURITY.md`](SECURITY.md).

### What we do

- **No network access** — nothing leaves your machine. No telemetry, no cloud
- **No surfaces of our own before the editor** — selection is the portal's; the app creates one ordinary window
- **PNG validation** — the capture must decode as a PNG, within 15360 × 8640 pixels
- **No clipboard file** — a copy stays in the app's memory and is served from there until something else is copied; nothing is written to disk

### What we don't do

- We don't encrypt screenshots at rest. Saved snips are standard PNGs
- We don't clear the clipboard after a timeout. The snip stays until you copy something else
- We don't sandbox the process beyond standard user permissions

### Reporting vulnerabilities

Do not open public issues for security reports. Use private reporting as documented in [`SECURITY.md`](SECURITY.md).

---

## Tech stack

| Component | Detail |
|-----------|--------|
| Capture | XDG Desktop Portal `Screenshot` via `ashpd`, as in cosmic-screenshot |
| UI | libcosmic (iced) |
| Export | `tiny-skia`, at the snip's native resolution |
| Clipboard | `wl-clipboard-rs` on the host; the window's own clipboard in a Flatpak (see ARCHITECTURE.md) |
| Language | Rust |

---

## Release history

See [`CHANGELOG.md`](CHANGELOG.md).

---

## Contributing

Pull requests are welcome. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for setup, workflow, and testing expectations.

If something breaks on your COSMIC setup, open an issue with the log:

```bash
RUST_LOG=debug cosmic-ext-snip
```

---

## License

MIT
