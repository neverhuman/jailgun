# Release Readiness

Jailgun is not released from local runtime state. A release candidate must be
reproducible from tracked source, generated contract artifacts, and CI evidence.

## Version Source

- Rust crate versions live in each `crates/*/Cargo.toml`.
- Node package versions live in `package.json` and workspace package manifests.
- User-facing changes are recorded in `CHANGELOG.md`.

## Required Evidence

Run these commands before tagging:

```bash
bash scripts/ci-doctor.sh
bash scripts/ci-local.sh
bash ops/ci/release.sh
bash ops/ci/jankurai.sh
```

The expected release artifacts are:

- `agent/repo-score.json`
- `agent/repo-score.md`
- `agent/jankurai-badge.svg`
- `agent/jankurai-badge.json`
- `target/jankurai/copy-code.json`
- `target/jankurai/ux-qa.json`

## Release Process

This release process is the tracked control surface for release readiness.

1. Start from a clean worktree except for intentional release artifacts.
2. Regenerate contracts and audit artifacts through the documented CI scripts;
   do not hand-edit generated outputs.
3. Run the required evidence commands above and inspect failures before tagging.
4. Tag only after release evidence, provenance, and rollback notes are current.

## Integrity and Provenance

Release CI must pin external GitHub Actions by full commit SHA, set job
timeouts, use workflow concurrency, and upload the generated audit, UX, and
copy-code evidence. Secrets are not required for validation.

## Rollback

Remote deploy rollback is handled by the Rust cleanup policy. Dirty checkouts,
missing `origin/main`, failed preservation refs, and failed receipt writes stop
the deploy. A failed release tag should be superseded by a new tag rather than
rewritten.
