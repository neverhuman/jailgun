# Testing and Proof Lanes

Use `agent/test-map.json` for the narrow proof route for a changed path. The
local parity entry point is:

```bash
bash scripts/ci-local.sh
```

## Lanes

- Rust: `bash ops/ci/rust.sh`
- Node: `bash ops/ci/node.sh`
- Security: `bash ops/ci/security.sh`
- Contracts: `bash ops/ci/contracts.sh`
- Rendered UX: `bash ops/ci/ux-qa.sh`
- Copy-code: `bash ops/ci/copy-code.sh`
- Release readiness: `bash ops/ci/release.sh`
- Audit: `bash ops/ci/jankurai.sh`
- Doctor: `bash scripts/ci-doctor.sh`

## Repair Errors

Agent-readable errors include `purpose`, `reason`, common fixes, `docs_url`,
and `repair_hint`. The purpose names the boundary that failed, the reason gives
the stable machine-readable cause, common fixes list the smallest likely local
repairs, `docs_url` points to this file or a more specific owner document, and
`repair_hint` names the next rerun command.

## Contracts

Contract artifacts are governed outputs. `bash ops/ci/contracts.sh` checks
schema and fixture drift. `bash ops/ci/contracts.sh --write` refreshes the
schema and fixtures from `scripts/generate-contracts.mjs`.

## Rendered UX

`bash ops/ci/ux-qa.sh` builds the dashboard and writes artifact-backed UX
evidence under `artifacts/ux-qa/` and `target/jankurai/ux-qa.json`. The report
records desktop and mobile viewports, page.screenshot-compatible screenshot
paths, aria-snapshot artifacts, visual review status, accessibility testing
summary compatible with axe-core, layout stability / CLS measurements, API mock
state coverage, design tokens, geometry checks equivalent to
getBoundingClientRect, and the batch-quality verdict surface in the run
header.

Batch request coverage lives in the Rust and dashboard lanes: the JMCP
batch envelope is validated by the Rust tests, and the cockpit quality
summary is exercised by the dashboard tests plus the rendered UX lane.

Browser profile-pool coverage lives in the Chrome bridge self-test,
orchestrator Rust tests, and dashboard tests. These lanes validate profile
rotation, per-profile state planning, event metadata mapping, and dashboard
rendering without opening real Chrome or requiring Google credentials.

`apps/fake-chatgpt` is the CI-owned ChatGPT copycat surface. New end-to-end
coverage for upload, prompt submission, tar discovery, and multi-profile
browser orchestration should target that fake service first. Real ChatGPT,
real Google profiles, and paid API calls remain operator-only smoke surfaces,
not default CI requirements.

## Budgets and Stop Conditions

CI lanes must not require real Google, GitHub write, SSH, Telegram, or browser
profile credentials. Paid or unbounded remote work stops when credentials are
missing, when a lane needs private runtime state, when a generated artifact
would require a hand edit, or when a command exceeds its GitHub job timeout.
Local agents should report the failing lane, the last artifact path, and the
next rerun command instead of broadening scope.

Cost-bearing work has a zero-default budget in CI: paid API calls are disabled
unless a reviewer records a budget, quota, spend cap, and kill switch for the
run. The stop condition is any missing quota evidence, any exhausted budget,
any missing kill-switch owner, or any lane that would need private runtime
credentials.

## Launch Gates

Release evidence must cover security scans, backup or preservation behavior,
monitoring and audit artifacts, rollback instructions, and abuse controls for
prompt/tool agency. The minimum local launch gate is `bash ops/ci/release.sh`
plus `bash ops/ci/jankurai.sh`; failures stop release work until the specific
artifact is repaired.
