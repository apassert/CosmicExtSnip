# Contributing to Snip for COSMIC™

Thanks for helping improve Snip for COSMIC™ (`cosmic-ext-snip`).

## Development Setup

```bash
git clone https://github.com/apassert/CosmicExtSnip.git
cd CosmicExtSnip
```

Install the libraries libcosmic links against (Pop!_OS / Ubuntu), then build
and check:

```bash
sudo apt install pkg-config libxkbcommon-dev libwayland-dev libfontconfig-dev libfreetype-dev
just check        # cargo test + cargo clippy --all-targets -- -D warnings
just run
```

## Debug Mode

```bash
RUST_LOG=debug cargo run
```

## Checking what the window renders

`cosmic-ext-snip some.png` opens an image in the editor without taking a snip.
With `COSMIC_EXT_SNIP_DUMP=/tmp/window.png` set, the editor saves its own
rendered frame - alpha included - once it has settled, and exits. That is how
to check what the app draws, independent of the compositor:

```bash
COSMIC_SINGLE_INSTANCE=0 COSMIC_EXT_SNIP_DUMP=/tmp/window.png cosmic-ext-snip small.png
```

## Commit Style

Use Conventional Commits where possible:

- `feat:` new feature
- `fix:` bug fix
- `docs:` documentation-only changes
- `refactor:` internal code cleanup without behavior change
- `chore:` maintenance and tooling updates

## Pull Request Process

1. Fork the repository.
2. Create a feature branch from `main`.
3. Keep each PR focused on one logical change.
4. Open the PR against `main` with a clear description and test notes.

## Code Style

- `cargo fmt` and `cargo clippy --all-targets -- -D warnings` must be clean.
- Keep drawing and export logic in the display-free modules (`annotation`,
  `render`) so it stays covered by `cargo test`.

## Testing Notes

`cargo test` covers the annotation model, the export renderer (pixel
assertions), portal URI handling and the fit transform. The portal and the
window need a real session, so also smoke-test on COSMIC Wayland:

1. Launch `cosmic-ext-snip` from the launcher or a shortcut.
2. Drag-select on single and multi-monitor layouts.
3. Confirm editor tools draw correctly (pen/highlighter/arrow/rect).
4. Verify copy (`Ctrl+C`) and save (`Ctrl+S`) flows.
5. Verify `Esc` in the portal and in the editor exits cleanly, and `Ctrl+N`.
6. Paste after the window has closed: the copy must still be there.

## Releasing

1. Set the version in `Cargo.toml` (and `cargo build` to update `Cargo.lock`),
   add a `<release>` entry at the top of the metainfo's `<releases>`, and a
   `## [x.y.z]` section to `CHANGELOG.md`. `scripts/release-check.sh` says
   whether they agree; the release workflow runs it too.
2. Merge that to `main`, then tag it: `git tag -a vX.Y.Z -m "..."` and
   `git push origin vX.Y.Z`.
3. `.github/workflows/release.yml` checks the tag against the version, runs
   the tests and clippy, and publishes a GitHub release with the CHANGELOG
   section as its notes, an x86_64 tarball, and `cargo-sources.json` for the
   Flathub manifest.

To try the pipeline without releasing: Actions -> Release -> Run workflow,
with `publish: none` (artifacts only) or `draft` (a draft release; delete it
afterwards, or the tag's release cannot be created). `runner: self-hosted`
needs a runner labelled `cosmicsnip-release`.

## Security Reports

Please do not file public issues for security vulnerabilities.

Follow the private reporting process in [SECURITY.md](SECURITY.md).
