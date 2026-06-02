# jankurai Repo Score

- Standard: `jankurai`
- Auditor: `1.4.3`
- Schema: `1.9.0`
- Paper edition: `2026.05-ed8`
- Target stack ID: `rust-ts-vite-react-postgres-bounded-python`
- Target stack: `Rust core + TypeScript/React/Vite + PostgreSQL + generated contracts + exception-only Python AI/data service`
- Repo: `.`
- Run ID: `1780361594`
- Started at: `1780361594`
- Elapsed: `22399` ms
- Scope: `full`
- Raw score: `85`
- Final score: `70`
- Decision: `advisory`
- Minimum score: `85`
- Caps applied: `fallback-soup-in-product-code, authz-or-data-isolation-gap`

## Hard Rule Caps

| Rule | Max Score | Applied |
| --- | ---: | --- |
| `no-root-agent-instructions` | 75 | no |
| `no-one-command-setup-or-validation` | 70 | no |
| `no-deterministic-fast-lane` | 65 | no |
| `no-security-lane-on-high-risk-repo` | 60 | no |
| `generated-contracts-or-public-api-drift-untested` | 80 | no |
| `python-direct-product-truth-or-db-ownership` | 72 | no |
| `no-secret-or-dependency-scanning-in-ci` | 78 | no |
| `no-jankurai-audit-lane-in-ci` | 82 | no |
| `jankurai-required-tool-ci-evidence-gap` | 88 | no |
| `non-optimal-product-language-found` | 74 | no |
| `too-much-python-in-product-surface` | 72 | no |
| `boundary-reclassification-evidence-gap` | 72 | no |
| `vibe-placeholders-in-product-code` | 68 | no |
| `fallback-soup-in-product-code` | 70 | yes |
| `future-hostile-dead-language-in-product-code` | 64 | no |
| `severe-duplication-in-product-code` | 70 | no |
| `generated-zone-mutation-risk` | 76 | no |
| `direct-db-access-from-wrong-layer` | 66 | no |
| `missing-web-e2e-lane` | 82 | no |
| `missing-rendered-ux-qa-lane` | 84 | no |
| `prompt-injection-risk` | 78 | no |
| `overbroad-agent-agency` | 65 | no |
| `secret-like-content-detected` | 60 | no |
| `false-green-test-risk` | 76 | no |
| `destructive-migration-risk` | 70 | no |
| `authz-or-data-isolation-gap` | 78 | yes |
| `input-boundary-gap` | 78 | no |
| `agent-tool-supply-chain-gap` | 78 | no |
| `release-readiness-gap` | 80 | no |
| `missing-rust-property-or-integration-tests` | 82 | no |
| `no-agent-friendly-exception-pattern` | 76 | no |
| `missing-agent-readable-docs` | 80 | no |
| `streaming-runtime-drift` | 78 | no |
| `rust-bad-behavior` | 72 | no |
| `sql-bad-behavior` | 72 | no |
| `typescript-bad-behavior` | 72 | no |
| `docker-bad-behavior` | 72 | no |
| `python-bad-behavior` | 72 | no |
| `ci-bad-behavior` | 70 | no |
| `git-bad-behavior` | 70 | no |
| `gittools-bad-behavior` | 70 | no |
| `release-bad-behavior` | 70 | no |
| `web-security-bad-behavior` | 68 | no |
| `repo-rot-bad-behavior` | 88 | no |
| `comment-hygiene-dangerous-residue` | 72 | no |
| `ci-local-parity` | 70 | no |

## Copy-Code Redundancy

- Status: `review` hard=`0` warning=`3` files=`68`
- Policy: min-lines=`10` min-tokens=`100` max-findings=`50` include-tests=`false` strict=`false`
- Duplicate volume: lines=`3` tokens=`13` bytes=`119`

- Notes:
  - hard classes are limited to exact active-source file matches and substantial exact same-name units
  - warning classes include same-body different-name units and token/block duplication
  - tests, fixtures, stories, config, Docker, and migrations are omitted unless --include-tests is set

| Kind | Severity | Language | Lines | Tokens | Instances | Reason |
| --- | --- | --- | ---: | ---: | --- | --- |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 11 | `crates/jailgun-server/src/lib.rs:594-595, crates/jailgun-server/src/lib.rs:636-637, crates/jailgun-server/src/lib.rs:656-657` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 0 | `crates/jailgun-core/src/agent_error.rs:35-36, crates/jailgun-deploy/src/deploy/model.rs:151-152, crates/jailgun-server/src/bus.rs:27-28` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 2 | `crates/jailgun-deploy/src/deploy/events.rs:216-217, crates/jailgun-orchestrator/src/run/mod.rs:882-883` | `same body appears under different names across files` |

## Dimensions

| Dimension | Weight | Score | Weighted | Evidence |
| --- | ---: | ---: | ---: | --- |
| Ownership and navigation surface | 13 | 83 | 10.79 | root `AGENTS.md` present; owner map present |
| Contract and boundary integrity | 13 | 88 | 11.44 | contract surface found; generated contract artifacts found |
| Proof lanes and test routing | 12 | 100 | 12.00 | one-command setup/validation lane found; deterministic fast lane found |
| Security and supply-chain posture | 12 | 100 | 12.00 | lockfile present; secret or dependency scan tooling found |
| Code shape and semantic surface | 12 | 27 | 3.24 | largest authored code file: crates/jailgun-cli/src/main.rs (1190 LOC); code file exceeds 500 LOC |
| Data truth and workflow safety | 8 | 100 | 8.00 | database surface present; structured db boundary manifest present |
| Observability and repair evidence | 8 | 88 | 7.04 | observability libraries or patterns found; ops/observability directory present |
| Context economy and agent instructions | 7 | 93 | 6.51 | root `AGENTS.md` present; root `AGENTS.md` stays short |
| Jankurai tool adoption and CI replacement | 7 | 100 | 7.00 | control-plane files present; applicable=17 |
| Python containment and polyglot hygiene | 4 | 100 | 4.00 | no Python files in scope |
| Build speed signals | 4 | 70 | 2.80 | build acceleration markers found; targeted test/build commands found |

## Reference Profile Structure

- Applicable cells: `3` canonical=`3` noncanonical=`0` guidance missing=`0`

| Cell | Status | Canonical | Detected | Aliases | Guidance | Owner | Proof lane | Agent fix |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `web` | `not_applicable` | `apps/web/` | `-` | `frontend/, ui/, packages/web/, packages/ui/` | `not_required` | `apps/web` | `rendered UX / Playwright` | `no action` |
| `api` | `not_applicable` | `apps/api/` | `-` | `api/, server/, backend/` | `not_required` | `apps/api` | `edge handler / contract tests` | `no action` |
| `domain` | `not_applicable` | `crates/domain/` | `-` | `domain/, core/` | `not_required` | `crates/domain` | `unit / property tests` | `no action` |
| `application` | `not_applicable` | `crates/application/` | `-` | `application/, usecases/, use-cases/` | `not_required` | `crates/application` | `use-case / authz tests` | `no action` |
| `adapters` | `not_applicable` | `crates/adapters/` | `-` | `adapters/, infra/, integrations/` | `not_required` | `crates/adapters` | `adapter integration tests` | `no action` |
| `workers` | `not_applicable` | `crates/workers/` | `-` | `workers/, jobs/, scheduler/, queue/` | `not_required` | `crates/workers` | `workflow / replay tests` | `no action` |
| `contracts` | `canonical` | `contracts/` | `contracts` | `openapi/, protobuf/, json-schema/, generated/` | `present` | `contracts` | `generation / drift checks` | `keep `contracts/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `db` | `canonical` | `db/` | `db` | `migrations/, constraints/, sql/` | `present` | `db` | `migration / constraint tests` | `keep `db/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `python-ai` | `not_applicable` | `python/ai-service/` | `-` | `python/, ai-service/, evals/, embeddings/, model/` | `not_required` | `python/ai-service` | `eval / contract tests` | `no action` |
| `ops` | `canonical` | `ops/` | `.github, .github/workflows, ops` | `.github/, .github/workflows/, ci/, release/, observability/, security/` | `present` | `ops` | `security lane / workflow lint` | `keep `ops/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |

## Rendered UX QA

- Web surface: `true`
- Layered UX lane: `true`
- Missing: `none`

### Ingested UX QA report (`target/jankurai/ux-qa.json`)
- Report count: `2`
- Worst decision: `pass`
- Total violations: `0`
- Summary errors / warnings: `0` / `0`
- Artifact counts: `accessibility=2, aria-snapshot=2, screenshot=2`
- Artifact fingerprints: `6`
- Visual baseline counts: missing=`0` changed=`0` review=`0` block=`0`
- Missing required states: `0` report(s) `none`
- Missing required artifacts: `0` report(s) `none`
- Accessibility violations / incomplete / passes: `0` / `0` / `24`

## Tool Adoption

- Control plane present: `true`
- Applicable tools: `17`
- Configured: `17`
- CI evidence: `17`
- Artifact verified: `17`
- Replaced count: `17`
- Missing CI evidence: `none`

| Tool | Category | Mode | Status | Replaced | Artifacts |
| --- | --- | --- | --- | --- | --- |
| `audit-ci` | `audit` | `advisory` | `artifact_verified` | `manual repo scoring, ad hoc score gates` | `agent/repo-score.json, agent/repo-score.md` |
| `proof-routing` | `proof` | `advisory` | `artifact_verified` | `ad hoc proof lane selection, manual proof receipts` | `agent/repo-score.json, agent/repo-score.md, target/jankurai/repair-queue.jsonl` |
| `proofbind` | `proof` | `auto` | `artifact_verified` | `manual changed-surface routing, ad hoc proof obligation lists` | `target/jankurai/proofbind/surface-witness.json, target/jankurai/proofbind/obligations.json` |
| `proofmark-rust` | `proof` | `auto` | `artifact_verified` | `line-only coverage review, manual in-diff mutation review` | `target/jankurai/proofmark/proofmark-receipt.json, target/jankurai/proofmark/proof-receipt.json` |
| `copy-code` | `audit` | `auto` | `artifact_verified` | `ad hoc copy-code review, manual duplication triage` | `target/jankurai/copy-code.json, target/jankurai/copy-code.md` |
| `security` | `security` | `advisory` | `artifact_verified` | `gitleaks, dependency review, SBOM/provenance` | `target/jankurai/security/evidence.json` |
| `ci-bad-behavior` | `security` | `advisory` | `artifact_verified` | `mutable workflow refs, secret echo/debug workflow checks, non-blocking security scans` | `target/jankurai/language-bad-behavior.log` |
| `git-bad-behavior` | `audit` | `advisory` | `artifact_verified` | `destructive git automation, force-push release scripts, hidden stash-based state` | `target/jankurai/language-bad-behavior.log` |
| `release-bad-behavior` | `release` | `auto` | `artifact_verified` | `manual release checklist, ad hoc tag and artifact review, manual provenance review` | `target/jankurai/language-bad-behavior.log` |
| `ux-qa` | `ux` | `advisory` | `artifact_verified` | `playwright, axe-core, visual baselines` | `target/jankurai/ux-qa.json` |
| `db-migration-analyze` | `db` | `auto` | `not_applicable` | `manual migration review` | `target/jankurai/migration-report.json` |
| `contract-drift` | `contract` | `advisory` | `artifact_verified` | `handwritten contract drift checks, openapi diff` | `agent/repo-score.json, agent/repo-score.md` |
| `rust-witness` | `rust` | `auto` | `artifact_verified` | `manual witness graphing` | `target/jankurai/rust/witness-graph.json` |
| `vibe-coverage` | `audit` | `auto` | `not_applicable` | `manual vibe-coding coverage spreadsheet` | `target/jankurai/vibe-coverage.json, target/jankurai/vibe-coverage.md` |
| `coverage-evidence` | `proof` | `auto` | `not_applicable` | `manual coverage report review, ad hoc mutation survivor review` | `target/jankurai/coverage/coverage-audit.json, target/jankurai/coverage/coverage-audit.md` |
| `authz-matrix` | `security` | `auto` | `artifact_verified` | `manual authz matrix review` | `agent/repo-score.json, agent/repo-score.md` |
| `input-boundary` | `security` | `advisory` | `artifact_verified` | `manual unsafe sink review` | `agent/repo-score.json, agent/repo-score.md` |
| `agent-tool-supply` | `security` | `advisory` | `artifact_verified` | `manual MCP/tool trust review` | `agent/repo-score.json, agent/repo-score.md` |
| `release-readiness` | `release` | `auto` | `artifact_verified` | `manual launch checklist` | `agent/repo-score.json, agent/repo-score.md` |
| `cost-budget` | `release` | `auto` | `artifact_verified` | `manual spend review` | `agent/repo-score.json, agent/repo-score.md` |

## Boundary Reclassifications

No audited runtime boundary reclassifications declared.

## Findings

1. `medium` `shape` `.`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:shape` `soft` confidence `0.76`
   Route: TLR `Entropy`, lane `fast`, owner `tools`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: `Code shape and semantic surface` scored 27 below the standard floor of 85
   Fix: split large or ambiguous authored code into smaller semantic modules with focused tests
   Rerun: `just fast`
   Fingerprint: `sha256:5dbdb57c9a15d1f66ceceb8c95cdeeb62c60f90c392d4028c3ffcb2bc4e16704`
   Evidence: largest authored code file: crates/jailgun-cli/src/main.rs (1190 LOC), code file exceeds 500 LOC, code file exceeds 1000 LOC, most code files stay under 300 LOC
2. `medium` `proof` `Justfile`
   Rule: `HLT-018-PERF-CONCURRENCY-DRIFT`
   Check: `HLT-018-PERF-CONCURRENCY-DRIFT:proof` `soft` confidence `0.76`
   Route: TLR `Verification`, lane `fast`, owner `ci-release`
   Docs: `docs/testing.md`
   Reason: `Build speed signals` scored 70 below the standard floor of 85
   Fix: add fast deterministic build/test targets, caches, and narrow proof lanes for agent iteration
   Rerun: `just fast`
   Fingerprint: `sha256:a256a7390d4b91a5b0a95d6f092e524c8f4080f27fe2b62e28cf0801343d0fef`
   Evidence: build acceleration markers found, targeted test/build commands found, locked dependency graph present, CI cache hint found
3. `medium` `context` `agent/owner-map.json`
   Rule: `HLT-003-OWNERLESS-PATH`
   Check: `HLT-003-OWNERLESS-PATH:context` `soft` confidence `0.76`
   Route: TLR `Context/setup`, lane `fast`, owner `repo-governance`
   Docs: `agent/JANKURAI_STANDARD.md#ownership-boundaries`
   Reason: `Ownership and navigation surface` scored 83 below the standard floor of 85
   Fix: tighten owner/test maps and root routing until agents can localize ownership without inference
   Rerun: `just fast`
   Fingerprint: `sha256:f22331131a2d75b4ff814289d01473fb3ab281f4b3c8ac366b63eb97cfe85438`
   Evidence: root `AGENTS.md` present, owner map present, test/proof routing map present, local `AGENTS.md` file(s)
4. `high` `context` `agent/owner-map.json`
   Rule: `HLT-003-OWNERLESS-PATH`
   Check: `HLT-003-OWNERLESS-PATH:context` `hard` confidence `0.88`
   Route: TLR `Context/setup`, lane `fast`, owner `repo-governance`
   Docs: `agent/JANKURAI_STANDARD.md#ownership-boundaries`
   Reason: path `.env.live-7tab.example` has no owner-map route
   Fix: add the narrowest stable prefix for this path to `agent/owner-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:07d5e436bb09bfec5a2ab483be7f4bba4c73281b200019abbf585c767bd750d7`
   Evidence: .env.live-7tab.example
5. `high` `proof` `agent/test-map.json`
   Rule: `HLT-004-UNMAPPED-PROOF`
   Check: `HLT-004-UNMAPPED-PROOF:proof` `hard` confidence `0.88`
   Route: TLR `Verification`, lane `fast`, owner `repo-governance`
   Docs: `agent/JANKURAI_STANDARD.md#proof-lanes`
   Reason: path `.env.live-7tab.example` has no test-map proof route
   Fix: add the narrowest stable prefix and runnable proof command to `agent/test-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:aa75117206f500a5d1bbbe7216f33393819c8df163bfd8bf810d26bdb70fdb74`
   Evidence: .env.live-7tab.example
6. `high` `vibe` `apps/dashboard/src/App.tsx:64`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `dashboard-ui`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: fallback soup detected in product code
   Fix: collapse fallback chains into explicit typed states with bounded retry policy, telemetry, and documented repair guidance
   Rerun: `just fast`
   Fingerprint: `sha256:055a7daac3c239d96f7f6bf50816223a0e02e3aca2ce1aa297c70ff2cabbacc8`
   Evidence: apps/dashboard/src/App.tsx:64 if (others.length === 0) return null;
7. `high` `security` `crates/jailgun-cli/src/main.rs:1175`
   Rule: `HLT-022-AUTHZ-ISOLATION-GAP`
   Check: `HLT-022-AUTHZ-ISOLATION-GAP:security` `hard` confidence `0.88`
   Route: TLR `Business truth`, lane `db`, owner `rust-cli`
   Docs: `docs/audit-rubric.md#top-level-risk-mapping`
   Matched term: `rls`
   Reason: authz/data isolation requires negative proof evidence
   Fix: add owner/non-owner authorization tests or RLS evidence for the touched data boundary
   Rerun: `just fast`
   Fingerprint: `sha256:b9ba8df40da33de885e0f0b175d3a2fe5afeafefdd9347dd065d0b988debbe8a`
   Evidence: fn infers_github_owner_repo_from_supported_remote_urls() {

## Policy

- Policy file: `./agent/audit-policy.toml`
- Minimum score: `85`
- Fail on: ``

## Agent Fix Queue

1. `high` `HLT-022-AUTHZ-ISOLATION-GAP` `crates/jailgun-cli/src/main.rs` - add owner/non-owner authorization tests or RLS evidence for the touched data boundary
   Route: `Business truth`/`db`
2. `high` `HLT-004-UNMAPPED-PROOF` `agent/test-map.json` - add the narrowest stable prefix and runnable proof command to `agent/test-map.json`
   Route: `Verification`/`fast`
3. `medium` `HLT-018-PERF-CONCURRENCY-DRIFT` `Justfile` - add fast deterministic build/test targets, caches, and narrow proof lanes for agent iteration
   Route: `Verification`/`fast`
4. `high` `HLT-003-OWNERLESS-PATH` `agent/owner-map.json` - add the narrowest stable prefix for this path to `agent/owner-map.json`
   Route: `Context/setup`/`fast`
5. `medium` `HLT-003-OWNERLESS-PATH` `agent/owner-map.json` - tighten owner/test maps and root routing until agents can localize ownership without inference
   Route: `Context/setup`/`fast`
6. `high` `HLT-001-DEAD-MARKER` `apps/dashboard/src/App.tsx` - collapse fallback chains into explicit typed states with bounded retry policy, telemetry, and documented repair guidance
   Route: `Entropy`/`fast`
7. `medium` `HLT-001-DEAD-MARKER` `.` - split large or ambiguous authored code into smaller semantic modules with focused tests
   Route: `Entropy`/`fast`
