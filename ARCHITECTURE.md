# Architecture

Snip for COSMIC™ (`cosmic-ext-snip`, formerly CosmicSnip) is cosmic-screenshot
with an annotation editor after it.

```
cosmic-ext-snip
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
        Copy  → render::composite → PNG → hand-off → window closes → main serves it
        Save  → portal file chooser → render::composite → write → exit
        Esc   → exit
        Ctrl+N → editor closed → portal again → a new editor window on the result
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

On Wayland the process that sets the clipboard must answer every paste, so a
copy lives exactly as long as the process that made it. How it is kept alive
depends on where the app runs:

- **Installed on the host:** the editor leaves the PNG in a hand-off slot and
  the app ends; `main` then serves it with `wl-clipboard-rs` (the data-control
  protocol) until another client takes the clipboard.
- **In a Flatpak sandbox:** COSMIC hides the data-control protocols from
  sandboxed clients, so the only clipboard is the window's own. Ctrl+C writes
  `image/png` through the window. iced ties its clipboard to one window, and
  closing that window drops the connection and the copy with it, so while a
  copy is held that window is minimised, never closed; if restored, it says
  what it is for. Closing it on purpose lets the copy go. A `clipboard:///` answer from the portal is read the same way, once
  the editor window has focus.

The app starts with no window - the snip is taken first, so the editor is
never in its own picture - and it is a single instance: launching it again,
like Ctrl+N, takes a new snip in the running process. That is what lets a
sandboxed copy survive: there is only ever one process, and it is still there.

## Windows on Wayland

A Wayland client can neither hide a window nor un-minimise one (winit-wayland:
"You can't unminimize the window on Wayland"; `set_visible`: "Not possible on
Wayland"). So an editor that has to leave the screen for a new snip is closed,
and a new window opens on the result; a cancelled selection reopens the
previous snip. The only window that is minimised instead is the one holding a
sandboxed copy (above).

The editor window is the snip at 1:1 plus the header, sized exactly when it
is created: content edge to edge (libcosmic's padded content box off), plus
libcosmic's 1 px window border and the header at the user's density (39 px
compact, 47 px standard). It has to be right at creation - COSMIC keeps a
floating window at the size it was mapped with, and both `window::resize` and
setting min = max afterwards were measured to change nothing. It opens pinned
(min = max) so COSMIC maps it floating even on a tiled workspace, and is
unpinned after its first frame so it can be resized. For Ctrl+C in the
selection the size is only known once the snip is read, so that window is
replaced by one of the right size. It is never narrower than the toolbar; a
narrower snip is centred on a transparent background. Its
*position* is the compositor's: a Wayland client cannot place a toplevel, and
the screenshot portal answers with the image only, not where the selection was.
