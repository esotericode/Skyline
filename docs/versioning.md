# Versioning and releases

The root `workspace.package.version` is the single application version; both crates
inherit it. The header, About window, window title and CLI expose that version.
Every different published testing/general build uses an unused version. Early work
increments 0.0.1, 0.0.2, 0.0.3; do not use a commit suffix to reuse a version for
different executable bytes. Larger capabilities and breaking pre-1.0 changes
advance the minor version. 1.0.0 marks the stable documented workflow.

Initial Actions builds used 0.0.1. Development artifacts also distributed 0.0.2
from 74f85a6 and 0.0.3 from 097518d0. The owner requested promotion of the latest
branch and a Windows GitHub Release. The 0.0.3 testing release promotes that existing
checked Windows artifact, preserving both executables byte-for-byte. Package README,
CHANGELOG and BUILD-INFO are corrected and checksums regenerated; this changes no
application code or binary. The tag points at the **built source 097518d0**, while
main additionally holds publication scripts and current documentation.

Routine main checks do not redistribute freshly rebuilt binaries under an already
used version. Manual testing-artifact distribution requires an unused version for
changed binary bytes. Documentation-only commits publish no new binaries and can
retain the version. The next different distributed build must be 0.0.4 or later.

Windows testing releases use `.github/windows-release.json` to pin an already
successful CI run, its exact source SHA, Windows artifact ID/name and SHA-256.
After both platform checks pass, a main commit explicitly marked `[release windows]`
runs `scripts/publish-windows-release.py` with job-scoped contents-write/actions-read
permissions. It checks provenance, the original archive and internal file checksums,
retains executable bytes, creates a deterministic `Skyline-vX.Y.Z-windows-x64.zip`
and SHA-256 sidecar, uploads to a draft, then publishes a prerelease. Existing tags or
assets with different evidence are refused, never overwritten. Expired artifacts
require a new reviewed build and unused version rather than silent recompilation.

The owner-authorized testing prerelease precedes completion of M5. Release notes
must state incomplete features, unsigned builds, native Windows GUI acceptance,
code-license selection and remaining notice review. Testing distribution does not
establish a clean Windows smoke test or complete toolkit/performance/accessibility
criteria. General release still requires those checks, completed operation/recovery
workflow, a chosen application license and complete dependency notices. Core tests
and successful compilation must never be reported as native GUI runtime acceptance.
