# Install the way cosmic-screenshot does: `just install` into ~/.local,
# `sudo just prefix=/usr install` system-wide.

name := 'cosmic-ext-snip'
appid := 'io.github.apassert.CosmicExtSnip'
prefix := env('HOME') / '.local'

bin-dst := prefix / 'bin' / name
desktop-dst := prefix / 'share' / 'applications' / appid + '.desktop'
icon-dst := prefix / 'share' / 'icons' / 'hicolor' / 'scalable' / 'apps' / appid + '.svg'
metainfo-dst := prefix / 'share' / 'metainfo' / appid + '.metainfo.xml'

default: build-release

build-release *args:
    cargo build --release {{args}}

check:
    cargo test
    cargo clippy --all-targets -- -D warnings

# In a COSMIC session: does a copy outlive the editor? (CONTRIBUTING.md)
copy-check: build-release
    cargo build --release --example clipboard_check
    scripts/copy-check.sh

run *args:
    cargo run --release -- {{args}}

install: build-release
    install -Dm0755 target/release/{{name}} {{bin-dst}}
    install -Dm0644 data/{{appid}}.desktop {{desktop-dst}}
    install -Dm0644 data/icons/hicolor/scalable/apps/{{appid}}.svg {{icon-dst}}
    install -Dm0644 data/{{appid}}.metainfo.xml {{metainfo-dst}}

uninstall:
    rm -f {{bin-dst}} {{desktop-dst}} {{icon-dst}} {{metainfo-dst}}
