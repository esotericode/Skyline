#!/usr/bin/env bash
# Packages the locked release build in target/release as a development build.
# Testing builds use the shared version and a source commit label
# (see docs/versioning.md). Usage: scripts/package-dev-build.sh <windows-x64|linux-x64> [out-dir]
set -euo pipefail

platform=${1:?platform: windows-x64 or linux-x64}
out=${2:-dist}
case "$platform" in
    windows-x64) exe=.exe ;;
    linux-x64) exe= ;;
    *) echo "Unknown platform: $platform" >&2; exit 2 ;;
esac

cli="target/release/skyline-cli$exe"
desktop="target/release/skyline-desktop$exe"
for binary in "$cli" "$desktop"; do
    [[ -f $binary ]] || { echo "Missing $binary; run cargo build --locked --workspace --release" >&2; exit 1; }
done

version=$("$cli" --version)
version=${version#Skyline }
commit=$(git rev-parse HEAD)
name="Skyline-v${version}-dev.${commit:0:8}-${platform}"
stage="$out/$name"
rm -rf "$stage"
mkdir -p "$stage"

cp "$desktop" "$stage/Skyline$exe"
cp "$cli" "$stage/skyline-cli$exe"
cp README.md CHANGELOG.md "$stage/"
cp docs/dependencies.md "$stage/THIRD-PARTY-DEPENDENCIES.md"
cp docs/dependency-inventory.tsv "$stage/"
cat > "$stage/BUILD-INFO.txt" <<EOF
Skyline development build
Version: $version (development testing build; no tagged GitHub Release)
Commit: $commit
Platform: $platform
Built: $(date -u +"%Y-%m-%d %H:%M UTC")
Toolchain: $(rustc --version)

Read-only prototype for multiple systems, loose cartridge files and local Logiqx XML DATs.
It never modifies ROM files. ZIP inspection and file organization are not implemented.
The application code license is pending; a full third-party notice bundle is still
required before any public release. Builds are unsigned.
EOF

(cd "$stage" && sha256sum -- * > SHA256SUMS.txt)

# Windows artifacts are uploaded as a folder (GitHub zips it once). Linux needs a
# tarball so executable permissions survive the artifact download.
if [[ $platform == linux-x64 ]]; then
    tar -C "$out" -czf "$out/$name.tar.gz" "$name"
    (cd "$out" && sha256sum -- "$name.tar.gz" > "$name.tar.gz.sha256")
    upload="$out/$name.tar.gz
$out/$name.tar.gz.sha256"
else
    upload=$stage
fi

echo "Packaged $name"
if [[ -n ${GITHUB_OUTPUT:-} ]]; then
    {
        echo "name=$name"
        echo "path<<PATHS"
        echo "$upload"
        echo "PATHS"
    } >> "$GITHUB_OUTPUT"
fi
