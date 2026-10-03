#!/usr/bin/env bash
#
# Is this tree a releasable version, and what are its release notes?
#
#   scripts/release-check.sh [notes-file]
#
# The version lives in four places - Cargo.toml, Cargo.lock, the metainfo's
# newest <release> and the CHANGELOG - and the store shows the metainfo one,
# so a tag cut while they disagree publishes a release the store describes
# wrongly. When run for a tag (GITHUB_REF_TYPE=tag), the tag must be
# v<version> too. Prints the version; writes the CHANGELOG section for it to
# notes-file.
set -euo pipefail

notes=${1:-/dev/null}
metainfo=data/io.github.apassert.CosmicExtSnip.metainfo.xml
fail=0
problem() { echo "release-check: $*" >&2; fail=1; }

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
[ -n "$version" ] || { echo "release-check: no version in Cargo.toml" >&2; exit 1; }

lock=$(awk '/^name = "cosmic-ext-snip"$/ { getline; gsub(/version = |"/, ""); print; exit }' Cargo.lock)
[ "$lock" = "$version" ] || problem "Cargo.lock has $lock, Cargo.toml $version"

newest=$(grep -m1 -o '<release version="[^"]*" date="[^"]*"' "$metainfo" || true)
meta_version=$(sed -n 's/.*version="\([^"]*\)".*/\1/p' <<<"$newest")
meta_date=$(sed -n 's/.*date="\([^"]*\)".*/\1/p' <<<"$newest")
[ "$meta_version" = "$version" ] || problem "the metainfo's newest release is '$meta_version', Cargo.toml $version"
today=$(date -u +%F)
[[ ! "$meta_date" > "$today" ]] || problem "the metainfo dates $version $meta_date, in the future"

section=$(awk -v v="$version" '
    $0 ~ "^## \\[" v "\\]" { on = 1; next }
    on && /^## \[/ { exit }
    on { print }' CHANGELOG.md)
[ -n "$(tr -d '[:space:]' <<<"$section")" ] || problem "CHANGELOG.md has no section for [$version]"

if [ "${GITHUB_REF_TYPE:-}" = tag ] && [ "${GITHUB_REF_NAME:-}" != "v$version" ]; then
    problem "tag ${GITHUB_REF_NAME:-} is not v$version"
fi

[ "$fail" = 0 ] || exit 1
printf '%s\n' "$section" | sed '/./,$!d' > "$notes"
echo "$version"
