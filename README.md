# CosmicSnip

Screenshot snipping tool with annotation for **COSMIC Desktop** on Pop!_OS 24.04.

The only capture + annotate tool that works natively on COSMIC's Wayland compositor.

---

## Project Transparency

- Version history: [`CHANGELOG.md`](CHANGELOG.md)
- Security policy and disclosure: [`SECURITY.md`](SECURITY.md)
- Contribution guide: [`CONTRIBUTING.md`](CONTRIBUTING.md)
- Architecture reference: [`ARCHITECTURE.md`](ARCHITECTURE.md)

---

## Why CosmicSnip?

COSMIC Desktop uses its own Wayland compositor, which breaks every existing screenshot tool:

| Tool | Problem on COSMIC |
|------|-------------------|
| `grim` / `slurp` | Requires `wlr-screencopy` — COSMIC doesn't expose it |
| `flameshot` | Crashes on COSMIC Wayland |
| `gnome-screenshot` | No Wayland support, X11 only |
| COSMIC built-in | Captures full screen only — no region select, no annotation |

CosmicSnip works because it asks the **XDG Desktop Portal** for the
screenshot, exactly as `cosmic-screenshot --interactive` does. COSMIC's own
portal draws the selection across every monitor; CosmicSnip then opens its
annotation editor on the result. It is a Rust application on
[libcosmic](https://github.com/pop-os/libcosmic), so it looks like the rest of
the desktop.

---

## Features

**Capture** (drawn by the COSMIC screenshot portal)
- Drag a region on any monitor, or across several
- `Enter` takes it, `Esc` cancels and CosmicSnip exits

**Annotate**
- Pen, highlighter, arrow, rectangle
- 6-colour palette and adjustable stroke width
- Undo (`Ctrl+Z`, up to 200 steps)
- Remembers your colour and stroke widths for the next snip; every snip starts with the pen

**Output**
- `Ctrl+C` copies the annotated snip at full resolution and closes; it stays
  on the clipboard after CosmicSnip has exited
- `Ctrl+S` saves a PNG, by default under `~/Pictures/Screenshots/`

### Keyboard shortcuts (editor)

| Key | Action |
|-----|--------|
| `P` `H` `A` `R` | Pen / Highlighter / Arrow / Rectangle |
| `+` / `-` | Thicker / thinner stroke |
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
git clone https://github.com/apassert/cosmicsnip.git
cd cosmicsnip
just install                 # into ~/.local
# or: sudo just prefix=/usr install
```

The first build compiles libcosmic and takes several minutes.

### Set up a keyboard shortcut

**COSMIC Settings → Keyboard → Custom Shortcuts → +**

| Field | Value |
|-------|-------|
| Name | CosmicSnip |
| Command | `cosmicsnip` |
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

CosmicSnip is designed to handle screenshots safely. Screenshots are sensitive data — they can contain passwords, tokens, personal information.

Security policy and coordinated disclosure: [`SECURITY.md`](SECURITY.md).

### What we do

- **No network access** — nothing leaves your machine. No telemetry, no cloud
- **No surfaces of our own before the editor** — selection is the portal's; CosmicSnip creates one ordinary window
- **PNG validation** — the capture must decode as a PNG, within 15360 × 8640 pixels
- **Short-lived clipboard file** — a copy is handed to the clipboard server through a file in the temp directory, which the server deletes as soon as it has read it

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
| Clipboard | `wl-clipboard-rs`, served by a detached `cosmicsnip --serve-clipboard` |
| Language | Rust |

---

## Release history

See [`CHANGELOG.md`](CHANGELOG.md).

---

## Contributing

Pull requests are welcome. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for setup, workflow, and testing expectations.

If something breaks on your COSMIC setup, open an issue with the log:

```bash
RUST_LOG=debug cosmicsnip
```

---

## License

MIT
