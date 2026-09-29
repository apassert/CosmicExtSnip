# Contributing to CosmicSnip

Thanks for helping improve CosmicSnip.

## Development Setup

```bash
git clone https://github.com/itssoup/cosmicsnip.git
cd cosmicsnip
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

1. Launch `cosmicsnip` from the launcher or a shortcut.
2. Drag-select on single and multi-monitor layouts.
3. Confirm editor tools draw correctly (pen/highlighter/arrow/rect).
4. Verify copy (`Ctrl+C`) and save (`Ctrl+S`) flows.
5. Verify `Esc` in the portal and in the editor exits cleanly, and `Ctrl+N`.
6. Paste after CosmicSnip has exited: the copy must still be there.

## Security Reports

Please do not file public issues for security vulnerabilities.

Follow the private reporting process in [SECURITY.md](SECURITY.md).
