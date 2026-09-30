# v0.2.0 release gap register

This is an implementation record, not a release-readiness claim. v0.2.0 has not
been accepted or published. The release requires the complete five-stage concept
workflow, installation without a source checkout, and live Linux/macOS evidence.

## Preserved baseline

- Implementation branch: `feat/public-release`, based on `30b25c1`.
- Public main observed through anonymous Git and REST: `093f01d`.
- The source branch has 27 commits not on that public main. They are retained
  in the implementation branch's ancestry, including advanced archive/deploy work.
- Original modified bridge and example pacing configuration were copied into
  the implementation worktree. The original checkout is untouched. Its patch,
  untracked bridge backup, refs, status, and stash inventory are preserved in
  ignored private workspace state; the backup is not a release input.
- HTTP-ingestion PR #1 remains a draft. Its implementation (`f72687d`) matches
  local `5349fba` except for local CI/telemetry additions. No second ingestion
  implementation is merged. Its generated UX ownership additions need retaining;
  its old generated audit scores do not establish acceptance for new source.
- The connected GitHub app reports push access, without administrative access.
  Shell and app views of repository contents have differed; publication must
  recheck the actual credential, remote ref, and artifact identities.

## Resumed implementation ledger

Baseline rechecked at `8222e8d` (tree `b271c1dbd66604fe5fa039bc7fb9eb8f00182a58`).
The release worktree was clean; the original checkout, stash and local-only refs
were retained. Public main is `093f01d167ddf24aa394cdee87cafdf1a0e142c5`;
PR #1 is open/draft at `db4e140f00e917bc4cb2df14abffca4710d0b93d`.
The `github` remote is readable; the local `origin` mirror currently returns
HTTP 500. The shell credential has neither push nor admin permission. Visible
rulesets are empty; branch-protection lookup returns 404, which does not establish
that no protection applies. No publication action has occurred.

Statuses apply to the exact behavior in each row. `implemented` is not proof;
`automated-verified` is not live or package acceptance. The historical record below
is retained separately and does not satisfy final-source gates. New local evidence
is under ignored `target/release-evidence/resumed-*`; the baseline inventory records
historical evidence hashes. No candidate archive was built in this continuation.
For unimplemented rows, implementation/evidence is explicitly absent. Commands
name owning lanes, not a claim that those lanes already exercise missing scenarios.

| Requirement | Expected behavior | Status | Implementation / owner | Regression and acceptance | Evidence identity / remaining dependency |
|---|---|---|---|---|---|
| R3.1 | Evidence-backed, revision-checked reconciliation across interfaces; hard kills never duplicate prompts | implementation-needed | — / workflow, orchestrator, interfaces | Rust, DB, E2E | —; reconciliation service, custody observations and hard-kill acceptance |
| R3.2 | Request-only identity; matching retries retain snapshots before resolving settings | automated-verified | `6a44291` / workflow, server | `cargo test --locked -p jailgun-workflow request_identity`; Rust, DB | baseline plus working diff; Linux x86-64; three regressions pass; final-source HTTP/MCP/package acceptance pending |
| R3.3a | Rotate eligible runs after each grant, including reductions and restart | automated-verified | `6a44291` / workflow | `cargo test --locked -p jailgun-workflow scheduling`; Rust, DB | baseline plus working diff; Linux x86-64; round-robin/restart regression passes |
| R3.3b | Check paused/auth-waiting deadlines; reject exhausted resume without releasing capacity | automated-verified | `6a44291` / workflow | scheduling regressions; Rust, DB | baseline plus working diff; Linux x86-64; deadline regression passes; UI guidance remains |
| R3.3c | Queue observations, adjustable effective capacity, independent slow browser operations, new-run recovery | implementation-needed | — / workflow, orchestrator, interfaces | Rust, DB, E2E, rendered UX | —; public types, operations and dashboard |
| R3.4 | Verified browser process/profile ownership and hard-crash repairs; actual two-account daemon restart | implementation-needed | — / core, orchestrator, bridge | Rust, Node, E2E | historical supervisor proof only; actual daemon/database restart and crash ownership pending |
| R3.5 | Distinguish auth, network and selector errors; durable model observations | implementation-needed | — / adapter, orchestrator | Node, E2E, live | —; fixture gaps and live identity/selectors |
| R3.6 | Checkpoints, bounded verified chunks, Markdown fidelity and substantive stage structure | implementation-needed | — / adapter, workflow | Node, Rust, E2E | —; transfer protocol, checkpoints and stage validation |
| R4.1a | Idempotent historical completed-summary import without fabricated concept stages | implementation-needed | — / workflow | DB, Rust | —; validated importer |
| R4.1b | Offline restore and journaled exclusive profile activation; restored credentials invalidated | implementation-needed | — / core, workflow, CLI | DB, E2E, package | offline restore exists; custody activation and credential invalidation absent |
| R4.2 | Previewed terminal-run deletion, recoverable cleanup and offline verified purge | implementation-needed | — / workflow, core, CLI, dashboard | DB, Rust, E2E, package | —; deletion jobs, tombstones, inventory and purge |
| R4.3 | Private instance/protocol discovery without probing an implicit port | implementation-needed | — / core, CLI, server | Rust, E2E | —; descriptor identity and startup changes |
| R4.4 | Typed states/actions/errors and generated public contracts | implementation-needed | — / workflow, contracts | contracts, Rust, Node | existing contracts retained; new types/enums pending |
| R4.5a | Shared reconcile, capacity, history, deletion, activation and purge operations | implementation-needed | — / interfaces | Rust, E2E, contracts | —; shared services and parity |
| R4.5b | Bounded MCP artifact chunks with integrity and per-request authorization | automated-verified | `b3f2c30` / workflow, server | Rust, contracts, Node, E2E | `6a44291` plus working diff; Linux x86-64; Unicode/range/integrity/scope/revocation regressions pass; independent SDK, final-package and live acceptance pending |
| R4.6 | Actionable recovery, persistent drafts, queue/history, downloads, logout and rotation | implementation-needed | — / dashboard, interfaces | Node, rendered UX, E2E | existing dashboard retained; listed interactions pending |
| R4.7 | Explicit archive root, private runtime defaults and atomic advanced submissions | implementation-needed | — / advanced, core | Rust, E2E | —; preserve existing destructive-boundary proofs |
| R5.1 | Three canonical native archives and exact-byte OS compatibility | implementation-needed | — / distribution | platform, package | —; Linux and native macOS candidates/acceptance |
| R5.2 | One prerequisite definition and comprehensive JSON doctor | implementation-needed | — / core, distribution | Rust, package | —; disk/filesystem/protocol/startup checks |
| R5.3 | Journaled installer repair and boundary fault injection | implementation-needed | — / distribution | package | existing verified installer/uninstall retained; repair and fault cases missing |
| R5.4 | Real package upgrade with backup, activation, migration and safe rollback | implementation-needed | — / distribution, workflow | package, DB | —; actual old-to-new acceptance |
| R5.5 | Isolated packaged synthetic provider using production browser/scheduler | implementation-needed | — / CLI, orchestrator, distribution | package, E2E | visual demo exists; installed synthetic runtime absent |
| R5.6 | Full workflows on clean Linux/macOS without development tools | implementation-needed | — / distribution | clean package acceptance | historical Linux basic container proof exists; full browser/macOS proof missing |
| R5.7 | Independent reproducible builds and truthful signing metadata | implementation-needed | — / distribution | two independent builds per target | —; digest comparison and signing report |
| R6.1 | Tested README first-run journey and public commands | implementation-needed | — / docs | docs, package, anonymous clone/download | concept README exists; release/public command acceptance absent |
| R6.2 | Complete platform, interface, recovery, security and release guides | implementation-needed | — / docs | docs, release | —; guides must follow implemented operations |
| R6.3 | Execute every documented workflow, links, schemas and screenshot provenance | implementation-needed | — / docs, CI | docs, E2E, package | existing links/JSON/help lane is partial |
| R6.4 | Private reporting, version policy, coordinated v0.2.0 metadata and compatibility | implementation-needed | — / docs, distribution | security, release | —; admin reporting settings and truthful acceptance matrix |
| R7.1 | Canonical lanes with retained required contexts and explicit Chrome | implementation-needed | — / CI | all lanes, hosted matrix | —; protection requirements inaccessible |
| R7.2 | Common exact-source/artifact evidence envelopes and negative-case validation | implementation-needed | — / CI | evidence verifier regressions | —; common envelope and allowlisted uploads |
| R7.3 | Release decision verifies exact source, packages, live evidence and statuses | implementation-needed | — / CI | release verifier negative cases | existing release lane remains `release_verified: false` |
| R7.4 | Cohesive refactoring and real audit integration; score >=95, zero findings/caps | implementation-needed | — / authored modules, CI | governance, copy-code, rendered UX | strict gate remains unmet; historical score is not current acceptance |
| R7.5 | Read-only PR jobs; trusted publisher binds attestations to accepted archives | implementation-needed | — / CI | workflow security, release | —; publisher permission unavailable |
| R8.1 | Complete automated acceptance matrix on final source | implementation-needed | — / all owners | all mapped lanes, platform | component proofs only; final integrated matrix absent |
| R8.2 | Live Linux/macOS acceptance on exact candidate bytes | implementation-needed | — / operator acceptance | private live scenario matrix | —; operator accounts and environments required |
| R8.3 | Protected merge, immutable accepted release, anonymous digest/install verification | implementation-needed | — / publisher | hosted pipeline, anonymous acceptance | —; write/admin access, accepted artifacts and live evidence required |

## Continuation verification and remaining gates

`6a44291` implements request-only idempotency with an atomic forward migration,
persistent grant rotation, and paused/authentication-wait deadline handling.
`b3f2c30` adds bounded MCP artifact reads, the accepted final artifact reference,
generated contracts and real-browser HTTP/stdio artifact reconstruction checks.
These changes do not complete the other rows above.

Executed on Linux x86-64 with Node 24.21.0 and sandboxed Google Chrome:

- Rust formatting, warnings-denied Clippy, locked workspace/all-feature tests;
  34 workflow/storage tests pass after these changes.
- Generated-contract drift, Node typechecks/tests/build, dependency/security
  scans, documentation examples/help checks and release-control checks pass.
- Thirty rendered desktop/mobile dashboard checks pass. The synthetic result
  screenshot was regenerated through the owning tool and visually reviewed.
- Five/ten-candidate MCP workflows pass over HTTP and stdio, including actual
  chunked final text retrieval, full length and digest verification. Five/ten CLI
  workflows pass. This is synthetic browser evidence, not live acceptance.

The first broad browser run selected downloaded Chromium, whose sandbox could
not start on this host. Selecting installed Google Chrome preserves sandboxing.
An intermediate browser run then exposed old eight-tool assertions; these were
corrected to nine and both MCP transports were rerun successfully. The initial
Node lane used system Node 26 and failed DOM storage assertions; the pinned Node
24 rerun passed. Keep failed logs alongside successful reruns; they are not
release evidence for final packages.

The artifact-stage dirty-worktree governance scan reports score 93, zero caps and seven
findings: the existing authored-code shape finding plus six generated-zone
findings despite executed contract/screenshot generation and drift checks. The
strict 95/zero-findings gate is unsatisfied. Full `proofbind verify` against the
public base fails while trying to read `assets/dashboard-results.png` as UTF-8.
The witness command executes and indexes seven crates; it does not substitute
for failed proof binding or missing coverage/mutation receipts.

Private logs and generated reports are under `target/release-evidence/` with
`resumed-*` and `artifact-*` prefixes; browser scenarios additionally emit their
own source/input/binary/browser digests under `target/concept-workflows/`.
No release archive digest, macOS acceptance, independent non-Rust SDK acceptance,
live-provider evidence, protected merge or public download acceptance exists for
this continuation. The publisher/admin credential and operator live environments
remain unavailable. Implementation-needed rows remain engineering work, not
external-access blockers.

Default audit verification now writes under `target/jankurai/audit/` and leaves
tracked reports/badges unchanged; explicit `--write` retains public report
generation. Hosted check names and the 95/zero-findings/zero-caps gate are retained
and read the fresh report. Required audit uploads fail when absent. This fixes
dynamic report self-feedback; it does not resolve authored-code findings or
complete the final release verifier. Two executed hash snapshots confirm no
tracked file changed during default audit execution. The final isolated report
is **90, one authored-code shape finding, zero caps**, so the strict release gate
still fails. Removing comments that merely named unexecuted proof commands did
not manufacture replacement proof. A scoped artifact-reader proofbind/proofmark
run executes but reports review required, with coverage and mutation evidence
unavailable; the full-tree PNG failure remains unresolved.

The [application release guide](release.md) now distinguishes application/schema
upgrade and rollback from advanced remote checkout deployment. Its required
activation and final-release gates remain explicitly pending.

## Historical findings and acceptance checks

| ID | Finding / required change | Implementation and regression | Release acceptance still required |
| --- | --- | --- | --- |
| B1 | Lease IDs collide within one process/second | UUID IDs; deterministic `same_process_same_second_release_preserves_other_run` | Durable per-attempt ownership and crash reconciliation |
| B2 | Repeated setup replaces ports, paths and limits | Preserve bindings; reject identity/port conflicts; concurrent registration test | Two live accounts, restart and reconnect |
| B2a | Browser isolation checks use separate runs on one provider identity | Opt-in synthetic provider cookies bind separate identities and conversation ownership; registered real-Chromium supervisor proof covers distinct profile/port allocations, independent expiry/cancellation, unaffected five-stage work and both cookies after browser restart | Final-source supported-platform and two-live-account acceptance |
| B3 | Registry read/modify/write races | File lock covering allocation/update/save; atomic private writes | SQLite account migration with original metadata backups |
| B3a | Account registration can leave an ownership marker after commit failure | Deferred-foreign-key commit failure reproduces the blocked retry; exact installation-owned regular markers are reused, foreign and symlink markers are rejected; existing profile data remains intact | Final-source account onboarding acceptance |
| B3b | Legacy migration follows linked profile-owner markers; dangling links can restore archive access | Both defects reproduced in regression tests; registration and migration share exclusive atomic owner publication with bounded regular-file reads and same-owner retries; competing/same-owner concurrency, permissions and retained-profile checks | Final-source migration and two-account acceptance on supported hosts |
| B4 | Runtime parents initially use default permissions | Create directories 0700 and files 0600; permission regression | All browser/login/persistence runtime surfaces |
| B5 | Chrome locks removed without ownership proof | Preserve locks and report `profile-locked`; bridge self-test | Rust browser supervisor, managed PID/profile verification, run-scoped recovery |
| D1 | Browser subprocess inherits unrelated credentials | Explicit inherited environment allowlist; Rust unit test | Operator-only explicit overrides and complete API authorization |
| D1a | NDJSON readers buffer unlimited input before checking line size | Rust and shared Node readers enforce the byte ceiling while reading; open-frame regressions fail before the fix and pass after it; CRLF/UTF-8/EOF boundaries tested; raw child stderr is drained without persisting page or login data | Final-source packaged browser and advanced workflow acceptance |
| F1 | Saving response text converts an error to success | Diagnostics retain original error; new turn-scoped provider adapter preserves Markdown and requires settled completion; 12 real Chrome fixture checks; durable Rust supervisor and accepted-turn recovery | Live-provider selector/model verification and final-source daemon acceptance |
| I1 | API failures silently use synthetic results | Explicit synthetic demo, four-screen concept dashboard, draft preservation through re-pairing, stable retry keys, runtime schema and artifact-integrity validation; component and real-browser regressions | Final-source dashboard and live-provider acceptance |
| T1 | UX report fabricates screenshots and accessibility | 30 executed desktop/mobile state checks; Chromium screenshots, DOM/keyboard/geometry checks, axe results, real hashes/timestamps | Final-source CI and supported platforms |
| T1a | Loading, service failure and mobile comparison obscure the next action | Shared design tokens, observed model text, one primary verification action, accessible table scrolling and reserved workspace; delayed-API CLS measurements with a real browser calibration control; before/after screenshot review | Final-source rendered checks and supported platforms |
| A1 | Reviewed findings lack a durable register | This document; private baseline evidence | Every remaining requirement implemented with proof |
| C1 | Login and reconnect are incomplete | Require a provider identity signal, reject anonymous composers/mismatches, persist verified identity binding, keep manual login active; durable account supervisor and explicit model confirmation; reconnect refreshes discovered models while retaining profile, port and completed results | Private Linux display, operator-cookie noVNC proxy, actual RFB keyboard login, expiry/cancellation and isolation proofs; live provider session/expiry/recovery acceptance remains pending |
| C1a | In-flight account probes can expire a newer reconnect session | Real Chromium regression reproduces the stale expiry; one account control lock orders readiness, reconnect, confirmation and cancellation, including failure callbacks; explicit reconnect always refreshes verification | Full browser lane and final-source live reconnect acceptance |
| D2a | Daemon startup writes a usable pairing code to process logs | Regression rejects startup secrets; pairing is issued only by an explicit authenticated request; setup retains its one-use operator link and advanced service uses the same pairing API | Real daemon restart, dashboard cookie/WebSocket checks and final-source package acceptance |
| D2b | Environment proxies receive credentials intended for a loopback daemon | Executed regressions reproduce both REST and MCP proxy requests; both clients explicitly disable proxies while retaining redirect refusal; real CLI and official SDK tests verify working local operations, zero proxy requests, non-owner read/mutation denial and token revocation | Final-source CLI/MCP and packaged-client acceptance |
| D2 | Private reads and MCP initialization are unauthenticated | Central gate, bearer compatibility, one-use pairing, HttpOnly sessions, Host/Origin checks, private credentials and daemon lock; persisted account-scoped automation tokens and permanent revocation; operator token UI with one-time secret display and revocation confirmation; Rust and real-server Chrome tests | Operator-cookie-only login viewer with Origin checks, active-session revalidation and bounded private transport; final-source/live acceptance pending |
| E1 | Workflow state lacks durable transactional storage | SQLite service with migrations, bounded writer, event replay, account import, verified backup/restore; authenticated HTTP adapter with artifact integrity checks; storage and server regression suites | Completed-summary migration, packaged daemon and live acceptance |
| E2 | Backup/restore lacks an operator command and can adopt a concurrently created destination | Offline CLI commands refuse active ownership and mismatched schemas; exclusive private directory creation, continuous writer ownership, verified copied database and incomplete-restore marker; subprocess completed-result, corruption, permission and no-overwrite regressions | Final-source packaged upgrade/rollback and supported platforms |
| E3 | Background daemon has no supported stop operation before backup/upgrade | Operator-only authenticated instance status/stop, local runtime identity and ownership verification, no PID signalling, idempotent offline stop; subprocess shutdown/backup/restart and HTTP authorization regressions | Final-source package and supported-platform acceptance |
| G1 | Concept pipeline absent | Persisted five-stage service, ten perspectives, weighted ranking validation, summaries and context limits; 5/10 candidate service and real Chrome HTTP/MCP tests | Packaged dashboard/CLI/MCP and live-provider acceptance |
| H1 | Account pacing/budgets/retries are not durable | Transactional reservations, actual-submission pacing, cooldowns, persistent budgets, explicit subset/retry, cancellation and conservative restart recovery | Final-source daemon acceptance, live timeout/cancellation and reauthentication acceptance |
| H2 | Stop button can disappear as generation completes, retaining cancelled capacity | Owned-turn streaming state is rechecked after click timeouts; failed stop promises may be retried; deterministic before/after unit reproduction, real Chrome obstruction regression and concurrent-run cancellation proof | Final-source interruption and cancellation acceptance |
| H3 | Active captures outlive the persisted run deadline; provider retry time is lost | Supervisor stops the owned conversation at run/response deadlines; partial capture retains the named reason; adapter passes observed retry timestamps into account-wide cooldowns while preserving submission uncertainty; four real Chromium scenarios verify the stop, persisted budget, cooldown and absence of duplicate submissions | Final-source and live acceptance |
| S1 | Advanced deploy can reset after preservation fails or checkout changes | Reproduced failures; verified refs/stashes and synced receipts required before reset, immediate checkout checks, shared mutation lock and no untracked deletion; executed shell regressions | Final-source platform CI; operator must keep unrelated writers out of the deployment checkout |
| I2 | CLI probe mistakes slow initialization or an advanced-only service for concept readiness | Delayed HTTP response reproduces the startup failure; bounded read-only probe retries and typed account-readiness validation; setup rejects workflow-unavailable | Final-source clean install and daemon reuse |
| J1 | Installed runtime depends on development checkout | Shared executable-relative asset lookup; installed default config and MCP auto-start/reuse regression outside the checkout with spaces; native Linux bundle with pinned Node, dependency notices/SBOM, checksum verification and atomic versioned installation; bootstrap build/install and integrated outside-checkout smoke proof with preserved upgrades | macOS bundles, clean-environment installation and uninstall |
| J3 | Optional native startup is missing | Private systemd-user and launchd definition generation, stable managed executable paths, explicit activation, no credentials in definitions; CLI rejection and native installed lifecycle proofs | Final-source native package lane on every supported host; macOS execution pending |
| J4 | Installer accepts a path that Node's module loader rejects | Real package failure and installer regression reproduced; reject backslashes/control characters in requested and resolved prefixes before activation, including symlink aliases; spaces and other quoted characters remain covered | Final-source packages on supported hosts |
| J5 | Outside-checkout smoke checks leave source and development tools present | Pinned Ubuntu container with only verified download/proof inputs, no network or host runtime; checks toolchain absence, missing-Chrome guidance, setup/pairing/dashboard/private API, credential reuse, shutdown and offline backup/restore; source/archive/image/tool hashes recorded | Full packaged synthetic browser workflows in clean Linux/macOS environments and live acceptance remain required |
| J6 | Managed application removal is missing and can race processes using another runtime | Shared process-use lock retained across upgrades; verified inventory removal with an interruption journal and no recursive deletion; core, actual-binary and installer regressions cover active use, corruption, links, unknown files and data retention | Final-source native and clean package removal on supported hosts; explicit runtime purge remains pending |
| J2 | README and supporting guides describe the old archive-first journey | Concept-first README, verified synthetic Chromium screenshot and capture record, workflow/advanced guides, corrected MCP credential behavior, executable documentation lane checking local links, JSON and CLI help | Public-main clean clone, public downloads and live onboarding on every supported platform |
| K1 | Public concept contracts missing | Rust-generated schemas, frontend types, standalone validators and actual-storage account/partial/subset/five-stage fixtures; HTTP, CLI, MCP and dashboard consumers | Remaining interruption/expiry transition fixtures and packaged-client acceptance |
| M1 | MCP is a limited compatibility handler | Official Rust SDK Streamable HTTP and stdio gateway, Rust-derived tool schemas, per-request scoped authorization/revocation, protocol and compatibility tests; real Chrome 5/10 workflows through both transports | Packaged-client and live-provider acceptance |
| T2 | CI evidence and security coverage incomplete | Node 24.21.0 pinned; npm lockfile advisory fixes; all lane includes E2E, database, packaging and governance | Required scanners, Cargo advisories resolved, source-bound release evidence |
| T3 | Validation workflows duplicate tool setup, retain unused write permission and omit platform packages | Shared pinned Node/Python/Rust setup and download-only caches; read-only workflow tokens; strict workflow/local-action scan; actual-schema zero-finding gate; Ubuntu 24.04/26.04 and macOS 15 ARM/Intel validation and package jobs with explicit artifact lists | Execute hosted matrix after GitHub writes are available; preserve existing check names until full protection settings can be read; final source-bound publication gate remains required |
| P1 | Public distribution and live acceptance absent | Pending | Protected PR merge, exact candidate packages, private live evidence, public download verification |

Proof logs are kept under the implementation lane's ignored
`target/release-evidence/`. A passing component check is not a passing release.
No real provider prompts or private browser profiles are used in these checks.

## Verification recorded during implementation

- A real Chromium regression changes the synthetic account's model list after a
  completed five-candidate workflow, expires login and reconnects through the
  existing watcher. It reproduced a stale one-model response, then passed after
  discovery was tied to the current login observation. Account/profile/port
  allocation, submission count and completed results remain intact, and the newly
  discovered model can be confirmed. A subsequent restart reuses authentication.

- The native Linux development archive includes executable-relative assets, pinned
  Node, compiled bridges/dashboard, dependency notices and an SBOM with Rust and
  JavaScript dependencies. The installer verifies checksums before extraction,
  rejects unsafe/linked archive members and activates a verified version directory
  atomically. Unit proofs retain old assets and runtime data across upgrades.
  Packaged binaries pass installation/reinstallation, corrupt-checksum rejection,
  daemon pairing, private API and credential-reuse checks from a path with spaces.
  The full local lane passes; the archive remains a dirty development build, and
  neither clean-environment nor live-provider acceptance is established.

- The managed Linux viewer uses per-account X authority, private RFB socket
  descriptors and operator-cookie WebSockets. Actual Xvfb/x11vnc checks reject
  wrong-account cookies, competing viewers and X TCP access; killing the owner
  stops its display. The noVNC proof logs in through the canvas with keyboard
  input, exercises cancellation/expiry/reconnect and completes five stages.
  Desktop/mobile axe and focus checks exposed and corrected an invalid ARIA
  label. Actual-size panning keeps small controls usable in a narrow viewport.

- Concept dashboard component regressions cover retry idempotency, draft and run
  selection through session expiry, criteria, explicit subsets, identity/model
  confirmation, safe Markdown, artifact corruption and scoped token revocation.
- Rendered screenshots exposed a skip-link fragment routing defect. A regression
  now asserts that keyboard navigation moves focus without changing the screen.
- The dashboard's real-browser fixture lane drives operator pairing, account
  connection, manual-login waiting, identity/model confirmation, five/ten
  candidates, all four reduction stages, ranking and verified final output.
  These checks use the synthetic provider; live ChatGPT remains unverified.

- CLI subprocess integration covers file-content submission, criteria, idempotency
  conflicts, durable controls, non-success for incomplete `--wait`, automation
  authorization, pairing, private partial exports and hash failures. Concurrent
  local clients start one daemon outside the checkout with spaces in paths;
  repeated setup reuses credentials and issues a new single-use code.
- Real Chrome five/ten-candidate workflows also run through the actual CLI with
  scoped automation credentials. Final Markdown, JSON results, `--wait` and
  manifest exports are checked against the captured artifacts.

- Rust formatting, Clippy with warnings denied, workspace and all-feature tests.
- Locked Node installation, typechecks, workspace tests, production build on
  Node 24.21.0.
- Thirty real dashboard browser states (fifteen per viewport), including pairing,
  with accessibility, keyboard, geometry, console and network checks.
- Production Rust server plus real sandboxed Chrome: pairing, cookie reuse,
  pairing-code replay rejection, and authenticated WebSocket upgrade.
- Twelve sandboxed Chrome checks against an interactive synthetic ChatGPT
  service: persistent login, account binding, model discovery/selection,
  streaming Markdown, partial/empty output, expiry, rate limits, and turn ownership.
  The Rust scheduler now drives the same adapter through a supervised account bridge.
- Full five- and ten-candidate real Chrome workflows against the synthetic provider:
  nine/fourteen submissions, persisted artifacts, observed model and accepted turns,
  account-wide pacing checked with a 100x fixture clock. Restarting browser and
  storage during an accepted turn preserves login and completes without duplicate
  submission. A concurrent-run check rejects cross-run browser mutation and
  cancels one run while another completes on the same browser. These tests do
  not establish live ChatGPT compatibility.
- Nine daemon checks cover startup outside the checkout, spaces in paths, private
  credentials, HTTP authorization, exclusive ownership, graceful SIGTERM,
  authenticated instance shutdown and restart.
- A real HTTP/browser proof now covers manual synthetic login, observed identity
  and model confirmation, all five concept stages, a verified final download,
  and a fresh browser identity observation after daemon/store restart.
- Official SDK clients complete five/ten-candidate workflows over Streamable HTTP
  and through `jailgun mcp` stdio, with real sandboxed Chrome against synthetic
  ChatGPT. Protocol logs stay on stderr. SDK tests cover negotiation, request errors,
  notifications, schemas, account scope, revocation and operator archive compatibility.
- HTTP regression checks cover idempotency conflicts, persistent pause/resume/cancel,
  restart/event replay, strict input fields, scheduler exclusion, access control,
  and registered artifact ownership/integrity/content isolation.
- Secret scan, npm audit, Cargo audit/deny, workflow linting, SBOM generation,
  contract drift, deploy fake-backend E2E, and the existing database boundary lane.
  The database lane now executes the SQLite workflow service tests and verifies
  the actual bundled SQLite version, including disk-full and artifact failure paths.
- Full-tree governance scanning is now explicit (`--full`) in the owning audit
  lane. Incremental scores did not expose all existing deficiencies. Shared
  design tokens, measured layout stability and generated event-fixture consumers
  now have executable checks. Authored-code shape and the final zero-finding
  release gate remain outstanding.
- The advisory governance tool has reported false positives for generated
  lockfile changes and cannot recognize every executed browser check. Generated
  lockfiles were produced by Cargo/npm. Its results are retained and do not
  substitute for release acceptance.

The shell GitHub identity was checked and has no push permission. The connected
GitHub app reports push permission but an actual Git blob upload was rejected
with HTTP 403, `Resource not accessible by integration`. No public branch, PR,
tag or release was created by this work. Contents write access or a deliberately
permissioned publisher is required; reported repository permissions alone do
not authorize publication.
