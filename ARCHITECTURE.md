# Architecture

CosmicSnip 2 is cosmic-screenshot with an annotation editor after it.

```
cosmicsnip
  │
  ├─ capture::request()        XDG Desktop Portal Screenshot
  │     interactive + modal    xdg-desktop-portal-cosmic draws the selection
  │                            on every output; Esc there → exit 0
  │     file:///….png   ─┐
  │     clipboard:///   ─┴─→  PNG bytes → render::decode_png → Pixmap
  │
  ├─ app::App (libcosmic)      one ordinary toplevel window
  │     header bar             tools, palette, width, undo, Save, Copy
  │     canvas Board           the snip fitted into the window, strokes on top
  │     keys                   P H A R, + -, Ctrl+Z/C/S/N/Q, Esc
  │
  └─ finish
        Copy  → render::composite → PNG → clipboard::spawn_server → exit
        Save  → portal file chooser → render::composite → write → exit
        Esc   → exit
```

## Why the portal draws the selection

CosmicSnip 1 captured the whole desktop and drew its own selection with one
GTK layer-shell overlay per monitor. That produced both bugs 2.0 fixes:
without `gtk4-layer-shell` the overlay covered one screen, and closing the
editor while those overlays were alive crashed, because COSMIC cannot destroy
layer-shell surfaces safely (the old `overlay.py` said so). The portal already
has a multi-monitor selection UI, so CosmicSnip no longer creates any surface
except its editor window.

## Modules

| Module | Display needed | Role |
|---|---|---|
| `annotation` | no | `Tool`, `Stroke`, `Document` with the undo history, arrowhead geometry |
| `render` | no | PNG decode/encode with a size limit, compositing strokes onto the snip with `tiny-skia` |
| `config` | no | palette and stroke defaults |
| `capture` | portal | the portal request and the `file://` / `clipboard:///` answers |
| `clipboard` | Wayland | serving a PNG after the editor has exited |
| `app` | window | the libcosmic application and the canvas program |

Strokes are stored in **snip pixel coordinates**. The canvas maps them to the
window with `app::Fit` (centred, never enlarged), and export composites them
at the snip's own resolution, so HiDPI captures keep every pixel.

## The clipboard

On Wayland the process that sets the clipboard must answer every paste. The
editor therefore writes the PNG to a temporary file and starts
`cosmicsnip --serve-clipboard <file>` in its own process group. That process
reads and deletes the file, serves `image/png` until another client takes the
clipboard, then exits. Copying twice leaves one server.
