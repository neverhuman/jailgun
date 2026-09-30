# CI Local Parity

`bash scripts/ci-local.sh` is the local entry point for the same shell lanes
used by GitHub Actions. `bash scripts/ci-doctor.sh` checks required tools
before running proof work.

The pinned Rust toolchain is `rust-toolchain.toml`. Node uses `npm ci` from the
tracked lockfile. CI jobs set explicit timeouts and concurrency so local proof
does not depend on stale or unbounded workflow runs.

The `all` lane includes Rust, Node, security, contracts, rendered dashboard UX,
duplicate-code review, documentation controls, executable documentation checks,
real-browser E2E, SQLite, native packaging and governance. Select an individual
lane with `bash scripts/ci-local.sh docs` (or `rust`, `node`, `e2e`, `db`,
`package`, `audit`, and the other names in the script).

Run `bash ops/ci/worker.sh` for the browser-free worker API proof. It uses only
mock executors and verifies MCP behavior without credentials or live setup.

`bash ops/ci/docs.sh` checks local Markdown file links, fenced JSON syntax and
30 actual CLI help surfaces. Its evidence explicitly does not claim that remote
links, section anchors, clean-clone installation or live-provider commands were
tested. Those need their own acceptance lanes. The README screenshot is copied
unchanged from a passing synthetic Chromium capture, with input hashes and
capture metadata. Regenerate it after dashboard changes with:

```bash
bash ops/ci/ux-qa.sh
node scripts/publish-doc-screenshot.mjs
bash ops/ci/docs.sh
```

Do not hand-edit generated screenshot records, schemas or audit scores. Keep
private profiles, tokens and raw provider transcripts out of uploaded artifacts.

## Hosted validation

The common setup action pins Node from `.node-version`, Python from
`.python-version` and Rust from `rust-toolchain.toml`. Dependency download cache
keys include platform, architecture and the relevant runtime/lockfile hashes.
Browser profiles, runtime state and compiled candidate binaries are not cached.
Validation workflows have only `contents: read`; no account credentials or
publication permissions are needed. External actions are pinned to full commits.

The platform matrix targets `ubuntu-24.04`, `ubuntu-26.04`, `macos-15` (ARM) and
`macos-15-intel`, matching the [official runner table](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
`bash ops/ci/platform.sh` executes Rust/Node, SQLite, actual browser workflows,
rendered UX, documentation and native package checks. The runner must provide
Chrome, or an explicit sandbox-capable `JAILGUN_TEST_CHROME`. Linux additionally
needs the display prerequisites installed in the workflow. A configured job is
not evidence of a passing hosted run; macOS/Ubuntu 26.04 acceptance remains pending.

The pinned actionlint 1.7.12 does not yet include Ubuntu 26.04 in its label table.
`.github/actionlint.yaml` adds only that verified hosted label using the linter's
additional-label mechanism. It suppresses no other finding and requires review
by 2026-12-31. Zizmor runs with the auditor persona and no ignore configuration;
the previous unused write permission and unnamed-job findings were repaired.

Existing workflow/job check names are retained until full branch-protection
requirements can be read with an authorized credential. Repeated setup code and
scanner installation use one implementation; compatibility workflow entrypoints
still execute their existing checks. Artifact uploads select synthetic evidence
and distribution archives explicitly, without uploading profile or runtime trees.
