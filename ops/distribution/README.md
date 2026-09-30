# Distribution tool pins

`.node-version` is the Node patch version used by CI, bootstrap, the bridge and
the shipped runtime. `node-checksums.txt` records the corresponding official
Node archive hashes from that version's
[`SHASUMS256.txt`](https://nodejs.org/dist/v24.21.0/SHASUMS256.txt).

`tools.json` pins esbuild, cargo-auditable and Syft. `tool-checksums.txt` records
the three supported Syft archive hashes from the
[upstream v1.40.0 checksum file](https://github.com/anchore/syft/releases/download/v1.40.0/syft_1.40.0_checksums.txt).
Bootstrap checks these hashes before extraction. Cargo installs cargo-auditable
with an exact version and `--locked`; the builder checks Cargo's installation
receipt and records the resolved executable's hash.

`clean-linux.json` pins the Ubuntu image and expected OS version for the isolated
installation check. The owning tool verifies Docker's actual image digest and
architecture, disables network access inside the test container, and records
that identity with the candidate archive and proof-tool hashes. Updating this
pin requires executing the clean installation proof again.

Update pins and lockfiles together, verify the upstream checksums, and run the
native package proof on all target platforms before accepting new tools. A
version pin alone is not evidence that a package works. Installer code, notices,
SBOMs, manifests and executable hashes are generated from the candidate source
and must remain associated with that candidate throughout publication.
