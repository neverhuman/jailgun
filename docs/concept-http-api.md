# Concept HTTP API

The development concept daemon runs the durable account supervisor and workflow
service with `jailgun serve`. The dashboard journey, packaged runtime,
server login viewer and live-provider acceptance are still in progress. A service
without the concept runtime returns `503` with `workflow-unavailable`.

All operations below require a bearer credential, `x-jailgun-token`, or a paired
dashboard cookie. Browser mutations require the same dashboard Origin. The
unauthenticated health endpoint exposes only `{"status":"ok"}`.

`POST /api/concept-runs` accepts JSON content, never a remote filesystem path:

```json
{
  "concept": "A neighborhood lending library",
  "account_id": "my-account",
  "candidate_count": 5,
  "constraints": "Use a small operating budget",
  "idempotency_key": "one-unique-key-per-request"
}
```

Candidate count defaults to five and allows five through ten. Criteria default
to usefulness 30, feasibility 25, novelty 20, supporting evidence 15 and risk
management 10. Supply `criteria` as an array of `{ "name": "...", "weight": 30 }`
objects to customize them; positive weights must total 100. Unknown fields,
deployment settings, bridge commands and prompt paths are rejected. The daemon snapshots the model selection explicitly confirmed during account
onboarding. Runs record the observed model for each accepted conversation.

Submission returns `202` with `run_id`, `status`, `run_url` and `result_url`.
Repeating the same key and content returns the existing run. Changing content
under that key returns `409` with `idempotency-conflict`.

| Operation | Response |
| --- | --- |
| `GET /api/service` | Operator-only daemon instance, private runtime path and running/stopping state |
| `POST /api/service/stop` | Operator-only JSON `{ "instance_id": "observed-instance-id" }`; `202` acknowledges graceful shutdown of that exact instance |
| `GET /api/accounts/sessions` | Login lifecycle, observed identity/models and confirmed preference |
| `POST /api/accounts/connect` | JSON `{ "email": "...", "id": "optional-account-id" }`; starts manual login |
| `POST /api/accounts/{id}/confirm` | Confirm the observed `identity` and `model` (`current` or `specific`) |
| `POST /api/accounts/{id}/reconnect` | Reopen the account login page without closing run conversations |
| `POST /api/accounts/{id}/cancel-login` | Cancel an active login session; preserve the profile |
| `GET /api/accounts` | Account readiness, capacity, active count and pacing times |
| `GET /api/runs` | Concept runs when the durable runtime is active |
| `GET /api/runs?kind=archive` | Advanced archive run snapshots |
| `GET /api/runs/{id}` | Stored run, configuration, stages and artifact metadata |
| `GET /api/runs/{id}/attempts` | Attempts, accepted turns, conversation links and observed models |
| `GET /api/runs/{id}/events?after=0` | Up to 1000 ordered events after the cursor |
| `GET /api/runs/{id}/result` | Completed result manifest; unfinished runs return `409` |
| `POST /api/runs/{id}/pause` | Pause future submissions; active capture continues |
| `POST /api/runs/{id}/resume` | JSON `{}` retries eligible work within the retained budget |
| `POST /api/runs/{id}/cancel` | Durable, idempotent cancellation retaining results |
| `GET /api/runs/{id}/artifacts/{artifact-id}` | Registered, integrity-checked download |

To explicitly continue with a subset, resume with `{"allow_incomplete":true}`.
At least three candidates must be complete, no attempt may remain active, and
comparison must not have started. The final manifest records exclusions. Partial
responses never count as complete candidates.

Downloads are attachments with `nosniff` and a sandbox policy. Markdown is served
as plain text so a browser cannot execute embedded HTML. Artifact identifiers
are scoped to the run; request paths cannot name arbitrary runtime files.

Errors contain `code`, `message` and `next_action`. Filesystem and SQL diagnostics
are excluded from public errors. Event consumers retain the latest sequence and
fetch again after it on reconnect; persistence commits precede visibility.

Canonical schemas are generated from Rust into
[`workflow.schema.json`](../contracts/json-schema/workflow.schema.json), with
shared [frontend types](../apps/dashboard/src/generated/workflow.ts). Run
`bash ops/ci/contracts.sh` to check drift or add `--write` to regenerate.

Account confirmation requires an identity observed within thirty seconds. A
specific model must appear in the observed selector or be its current selection.
Initial account IDs, capacity and profiles are preserved on repeated connection.
An anonymous composer never authorizes submission. Login observation expires
after fifteen minutes; no password or verification code enters this API.

For development, build the dashboard and CLI, then run from a checkout with an
explicit Node 24 executable and a separate private runtime:

```bash
jailgun serve --runtime "$HOME/.jailgun-development" \
  --assets "$PWD" --node "$(command -v node)" --addr 127.0.0.1:8787
```

This opens headed Chrome during connection. Headless mode is intended for an
already authenticated profile or deterministic browser tests until the server
login viewer is implemented. The installed asset layout resolves relative to
the executable; distribution bundles are not yet published. The existing
advanced `serve --live` mode remains separate.
