#!/usr/bin/env python3
"""Publish an already checked Windows build; executable bytes are never rebuilt."""
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import subprocess
import tomllib
import zipfile

REPO = "esotericode/Skyline"
ROOT = Path(__file__).resolve().parent.parent


def gh(*args):
    return subprocess.check_output(["gh", *args])


def api(endpoint):
    return json.loads(gh("api", f"repos/{REPO}/{endpoint}"))


def sha(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def publish():
    config = json.loads((ROOT / ".github/windows-release.json").read_text())
    version = config["version"]
    commit = config["source_commit"]
    tag = f"v{version}"
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())
    require(cargo["workspace"]["package"]["version"] == version, "Version mismatch")
    run = api(f"actions/runs/{config['run_id']}")
    require(
        run["status"] == "completed" and run["conclusion"] == "success"
        and run["head_sha"] == commit,
        "The selected source has not passed the complete build workflow",
    )
    artifact = api(f"actions/artifacts/{config['artifact_id']}")
    require(
        not artifact["expired"] and artifact["name"] == config["artifact_name"]
        and artifact["workflow_run"]["id"] == config["run_id"]
        and artifact["workflow_run"]["head_sha"] == commit
        and artifact["digest"] == "sha256:" + config["artifact_sha256"],
        "Artifact provenance does not match the selected build",
    )
    original = gh("api", f"repos/{REPO}/actions/artifacts/{config['artifact_id']}/zip")
    require(sha(original) == config["artifact_sha256"], "Artifact ZIP checksum failed")
    contents = {}
    with zipfile.ZipFile(io.BytesIO(original)) as archive:
        require(sum(i.file_size for i in archive.infolist()) <= 512 * 1024 * 1024,
                "Unexpected artifact size")
        for item in archive.infolist():
            if item.is_dir():
                continue
            path = PurePosixPath(item.filename)
            require(not path.is_absolute() and ".." not in path.parts,
                    "Unsafe artifact member path")
            require(item.filename not in contents, "Duplicate artifact member")
            contents[item.filename] = archive.read(item)
    # upload-artifact strips the staging folder. Require the expected root files.
    required = {"Skyline.exe", "skyline-cli.exe", "BUILD-INFO.txt", "SHA256SUMS.txt"}
    require(required <= contents.keys(), "Expected Windows package files are missing")
    expected = {}
    for line in contents["SHA256SUMS.txt"].decode().splitlines():
        digest, name = line.split(maxsplit=1)
        expected[name.lstrip("*")] = digest
    require(set(contents) - {"SHA256SUMS.txt"} == expected.keys(),
            "Artifact checksum manifest does not cover the package")
    for name, digest in expected.items():
        require(sha(contents[name]) == digest, f"Checksum failed for {name}")
    build_info = contents["BUILD-INFO.txt"].decode()
    require(f"Version: {version} " in build_info and f"Commit: {commit}" in build_info,
            "Embedded build provenance differs")
    binaries = {name: sha(contents[name]) for name in ("Skyline.exe", "skyline-cli.exe")}
    for name in binaries:
        require(contents[name].startswith(b"MZ"), f"{name} is not a Windows executable")

    # Correct package documentation only. These EXEs are the original tested bytes.
    contents["README.md"] = (ROOT / "README.md").read_bytes()
    contents["CHANGELOG.md"] = (ROOT / "CHANGELOG.md").read_bytes()
    contents["BUILD-INFO.txt"] = (
        f"Skyline Windows testing release\nVersion: {version}\nCommit: {commit}\n"
        f"Platform: windows-x64\nCI run: {config['run_id']}\n"
        f"Original artifact: {config['artifact_id']}\n"
        f"Original artifact SHA-256: {config['artifact_sha256']}\n\n"
        "Executable bytes are unchanged from the checked development build.\n"
        "Read-only loose/ZIP ROM verification with automatic DAT-folder intake.\n"
        "No file organization, copying, moving or recovery workflow yet.\n"
        "Native Windows GUI acceptance remains pending. Builds are unsigned.\n"
        "Application code license remains pending owner selection. The dependency\n"
        "inventory is included; full dependency-notice review remains open.\n"
    ).encode()
    contents["SHA256SUMS.txt"] = "".join(
        f"{sha(contents[name])}  {name}\n"
        for name in sorted(contents) if name != "SHA256SUMS.txt"
    ).encode()
    for name, digest in binaries.items():
        require(sha(contents[name]) == digest, f"Release changed the bytes of {name}")
        print(f"{name}: {digest}")

    out = ROOT / "dist"
    out.mkdir(exist_ok=True)
    name = f"Skyline-v{version}-windows-x64"
    package = out / f"{name}.zip"
    with zipfile.ZipFile(package, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for filename in sorted(contents):
            # Fixed timestamp makes retrying publication produce the same archive.
            info = zipfile.ZipInfo(f"{name}/{filename}", (2026, 10, 13, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, contents[filename])
    checksum = out / f"{name}.zip.sha256"
    checksum.write_text(f"{sha(package.read_bytes())}  {package.name}\n")
    notes = (ROOT / "docs/releases/0.0.3.md").read_text()
    notes += f"\nBuilt source: `{commit}`. CI: https://github.com/{REPO}/actions/runs/{config['run_id']}.\n"
    notes += "\nExecutable SHA-256:\n\n" + "\n".join(
        f"- `{filename}`: `{digest}`" for filename, digest in binaries.items()
    ) + "\n"
    notes_file = out / "release-notes.md"
    notes_file.write_text(notes)

    # Refuse to replace a published asset or reuse a tag for different source.
    pages = json.loads(gh("api", "--paginate", "--slurp", f"repos/{REPO}/releases"))
    existing = next((r for page in pages for r in page if r["tag_name"] == tag), None)
    assets = [package, checksum]
    if existing:
        for asset in assets:
            published = next((a for a in existing["assets"] if a["name"] == asset.name), None)
            require(published and published["digest"] == "sha256:" + sha(asset.read_bytes()),
                    f"Release already exists with different or missing {asset.name}; not replaced")
        require(existing["target_commitish"] == commit, "Existing release source differs")
        if not existing["draft"]:
            require(existing["prerelease"], "Existing release is not a testing prerelease")
            print(existing["html_url"])
            return
    else:
        refs = json.loads(gh("api", "--paginate", "--slurp",
                             f"repos/{REPO}/git/matching-refs/tags/{tag}"))
        require(not any(r["ref"] == f"refs/tags/{tag}" for page in refs for r in page),
                "Tag already exists without this release; not reused")
        gh("release", "create", tag, *map(str, assets), "--repo", REPO,
           "--target", commit, "--title", f"Skyline {version} — Windows testing release",
           "--notes-file", str(notes_file), "--prerelease", "--draft")
    # Assets must all upload successfully before the release becomes public.
    gh("release", "edit", tag, "--repo", REPO, "--draft=false", "--prerelease", "--latest=false")
    released = api(f"releases/tags/{tag}")
    require(not released["draft"] and released["prerelease"], "Release not published as prerelease")
    ref = api(f"git/ref/tags/{tag}")
    require(ref["object"]["sha"] == commit, "Release tag source differs")
    for asset in assets:
        uploaded = next((a for a in released["assets"] if a["name"] == asset.name), None)
        require(uploaded and uploaded["digest"] == "sha256:" + sha(asset.read_bytes()),
                f"Uploaded asset checksum differs: {asset.name}")
    print(released["html_url"])


if __name__ == "__main__":
    publish()
