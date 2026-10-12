# Versioning and releases

workspace.package.version in the root Cargo.toml is the single application version.
Both crates inherit it. Core VERSION embeds it; the desktop header/About/window
title and skyline-cli --version expose it. CI checks that crate versions agree.

The initial development version is 0.0.1. Every published testing or general
release increments to an unused version: 0.0.1, 0.0.2, 0.0.3 during early work.
Never publish different binaries under a reused version. Larger capabilities
increment the minor version (0.1.0, 0.2.0); 1.0.0 marks the stable documented workflow.
Breaking pre-1.0 changes increment the minor version.

Before publishing: bump Cargo.toml and refresh Cargo.lock; update CHANGELOG.md;
pass locked format/test/lint/release-build checks; record Windows smoke-test results;
resolve code licensing and dependency notices; tag the reviewed commit vX.Y.Z;
name binaries Skyline-vX.Y.Z-windows-x64; include checksums, commit and limitations;
verify the built About window and CLI display X.Y.Z. Testing releases may have
incomplete milestones only when their release notes clearly state those limits.

Initial testing builds used 0.0.1. The DAT-folder development change advances
source to 0.0.2; it does not itself create a tag, installer or testing distribution. Routine CI checks source commits and does not
publish distinct releases. Do not claim a checklist passed because a build compiled.

Development testing builds: CI on `main` (or a manual run) packages checked binaries
as Actions artifacts named `Skyline-vX.Y.Z-dev.<commit>-<platform>` with build info
and checksums. The commit identifies the source; it does not replace the owner's
incrementing version requirement. Before distributing a different testing build,
bump the shared workspace version to the next unused value and refresh Cargo.lock;
the UI and CLI must show that new version. The initial distribution uses 0.0.1.
Documentation-only commits can retain the version when they publish no new binaries.
An artifact need not have a tag, GitHub Release or installer to count as a testing
distribution. A tagged/general release still follows the release checklist above.
