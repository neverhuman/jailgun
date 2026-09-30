# Third-party distribution notices

Jailgun's own source uses the [MIT license](../LICENSE). Dependencies retain
their respective licenses. Release bundles must include dependency notices and
the source materials required by those licenses; the package lane verifies the
actual bundle contents before publication.

The embedded login viewer uses unmodified
[@novnc/novnc 1.7.0](https://github.com/novnc/noVNC/tree/v1.7.0), primarily under
the Mozilla Public License 2.0. Its npm distribution includes `AUTHORS`,
`docs/LICENSE.MPL-2.0`, additional license notices in `docs/`, and the original
`core/` and `vendor/` source. Preserve these in the installed notices/source
directory alongside the compiled dashboard. The viewer is loaded only when
an operator opens a managed Linux login session.

Xvfb, x11vnc and Google Chrome are separately installed system dependencies;
Jailgun does not currently redistribute them. The pinned Node runtime and its
LICENSE, and the compiled browser bridge's production dependency notices, must
also be included in release bundles. SBOM generation records the resolved
dependency versions. Native Linux package checks run in `bash ops/ci/package.sh`. macOS distribution
checks and publication remain pending on the v0.2.0 development branch.

The security lane's source SBOM scans a fresh snapshot selected by Git, including
lockfiles and non-ignored working changes. It excludes ignored profiles, local
configuration, build outputs and test installations, and rejects symlinked
inputs. Its receipt records the exact source hashes and tool version. This source
inventory is separate from the SBOM generated from each actual release bundle.
Only the latter describes distributed components.
