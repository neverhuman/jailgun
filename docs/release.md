# Application release procedure

v0.2.0 is incomplete. The [completion ledger](public-release-progress.md) separates
implementation, automated proof, live acceptance and publication. Do not tag or
publish while any required release gate is unmet. `ops/ci/release.sh` currently
checks documentation terms and files only and reports `release_verified: false`.
Its rollback metadata describes the required procedure, not an automated upgrade operation.

The DREAM worker fleet is a Linux-only delivery with required Ubuntu 24.04 and
26.04 CI lanes. macOS is outside that worker rollout's acceptance scope. The
broader application publication gates below remain separate and incomplete;
merging or deploying the worker does not approve a public multi-platform release.

## Version Source

Coordinate the version in Rust crate manifests, Node workspace manifests,
lockfiles, installer/package metadata, examples and `CHANGELOG.md`. Regenerate
lockfiles with their owning tools. Keep Node pinned to `.node-version` throughout
build and acceptance. A development archive is not a published v0.2.0 release.

## Required Evidence

Start with a clean candidate commit. Record its full commit/tree, tool versions
and every candidate archive digest. Local and hosted jobs must use the same
[proof lanes](testing.md) and [ownership map](../agent/test-map.json).

```bash
bash scripts/ci-doctor.sh
bash scripts/ci-local.sh
bash ops/ci/release.sh
bash ops/ci/jankurai.sh
```

The audit writes `target/jankurai/audit/repo-score.json` and its Markdown report
without changing tracked source. The hosted strict gate consumes that fresh JSON.
`bash ops/ci/jankurai.sh --write` explicitly regenerates tracked public reports
and badges when preparing documentation; those snapshots are historical evidence.

These commands alone do not approve publication. The final gate must require:

- Passing Rust, Node, contract, database, browser, rendered dashboard, security,
  documentation, distribution and governance evidence from the candidate source.
- Governance score at least 95, zero findings and zero caps; an advisory exit zero
  does not satisfy that gate. Keep generated-source checks and proof obligations.
- One canonical Linux x86-64 archive built on Ubuntu 24.04, with the same digest
  tested on Ubuntu 24.04 and 26.04; native macOS ARM and Intel archives tested on
  macOS 15 plus compatibility smoke checks on the current supported runner.
- Clean installed-package workflows, actual old-to-new upgrade/rollback proof,
  and independent repeat builds with explained digest differences.
- Private live Linux/macOS acceptance of identity, model selection, five/ten
  candidates, restart recovery, cancellation, reauthentication, two accounts and
  both MCP transports on those exact packages. Synthetic checks remain separate.

Evidence must identify executed scenarios, source inputs, platform, tool hashes,
command arguments, result, omissions and output hashes. Package tests must name
the archive digest. Reject missing, skipped, stale or mismatched required evidence.
Upload only sanitized allowlisted records, never whole runtime directories.
`agent/repo-score.*` and historical logs alone cannot establish these conditions.

## Candidate and publication sequence

1. Include all intended implementation and useful PR #1 ownership/proof metadata
   in a reviewable PR. Inspect actual protection requirements with an authorized
   publisher and satisfy them; preserve existing check contexts until then.
2. Merge through repository protections. Run the release pipeline against that
   exact merged commit and build the three canonical archives. Test each archive
   as a complete installation, without source/toolchain dependencies.
3. Create a draft release containing those candidate bytes. Attach full checksums,
   per-platform SBOMs, provenance, signing declarations, sanitized acceptance
   metadata and release notes. Sign before final hashes and acceptance if signing
   is configured. Report unsigned/ad-hoc and unnotarized status honestly.
4. Obtain administrator configuration for immutable releases and tag protection.
   Verify all automated, package and live gates against the draft asset digests.
5. Publish the accepted draft without rebuilding or replacing accepted assets.
   Verify remote source/tag identities and every anonymous download digest.
6. Test anonymous clone instructions and checksum verification, installation and
   packaged synthetic workflows from the public downloads. Record publication
   evidence in the ledger. Missing publisher/admin access leaves publication pending.

No final release verifier, complete platform acceptance or publication is claimed
by this guide. Those remain tracked implementation/acceptance requirements.

## Upgrade and activation

Keep the managed application prefix separate from the private runtime. Identify
all runtimes using the installation, pause or finish work, and gracefully stop
native startup jobs and daemons. Use the currently installed binary to create and
verify a backup **before** activating a new application or opening its database
with a new binary. The existing offline operations are documented in the
[CLI guide](cli.md#offline-backup-and-restore); browser profiles require their own backup
while browsers are stopped and are excluded from the workflow backup.

Verify and stage the new archive separately, then explicitly activate it when no
old daemon is using the installation. Start the new binary, apply forward
migrations and verify account/result availability before reenabling startup jobs.
Do not consider version-directory fixture tests proof of an actual package upgrade.
The complete coordinated upgrade/repair operation is still pending.

Restoration currently creates a verified offline runtime. It is not authorization
to launch browsers against profiles owned by the original runtime. Journaled
profile-custody activation, original-runtime refusal, fresh operator credentials
and invalidation of restored automation tokens must be implemented and tested
before claiming restored-runtime activation. Do not manually remove ownership
markers to bypass this missing gate.

## Release Process

This release process is the tracked control surface for release readiness.

1. Start from a clean worktree except for intentional release artifacts.
2. Regenerate contracts and audit artifacts through the documented CI scripts;
   do not hand-edit generated outputs.
3. Run the required evidence commands above and inspect failures before tagging.
4. Tag only after release evidence, provenance, and rollback notes are current.

## Integrity and Provenance

PR jobs remain read-only and credential-free. Trusted publication jobs receive
only the permissions needed to attach/attest candidate assets and publish the
accepted draft. Publication credentials are confined to trusted workflows. Pin
Actions by commit SHA, use bounded timeout and concurrency settings, and fail
required uploads when evidence is missing. Bind attestations and SBOMs to the
actual archive digests and source. Keep timestamps outside reproducible payload
identity where necessary.

## Rollback

Stop the new daemon and its startup job. Retain the failed runtime and its evidence.
Select the previous verified application and restore its matching schema backup
into a **new** private runtime. Never open a newer schema with an older binary or
attempt an untested down migration. Profile custody must transfer exclusively
through the activation operation described above, with interrupted-transfer
recovery; until that operation exists, automatic restored-profile activation is
not supported. Verify retained results and authentication before restarting jobs.

Application uninstall preserves runtime data by default. Runtime purge is a
separate, explicit operation and is not yet implemented. Keep external backups.
Advanced remote deployment rollback has separate
[checkout preservation guarantees](advanced-deploy-safety.md); it does not roll
back the installed application or workflow database. Published accepted releases
must be superseded, never rewritten in place.
