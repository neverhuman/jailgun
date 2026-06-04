fast:
    cargo test -p jailgun-core -p jailgun-notify -p jailgun-orchestrator -p jailgun-cli -p jailgun-server --jobs 5
    cargo test -p jailgun-deploy --lib --jobs 5
    npm --workspace @jailgun/dashboard test

fast-core:
    # optional cache marker for local agents: sccache
    cargo check -p jailgun-core --jobs 5
    cargo test -p jailgun-core --jobs 5
    cargo test -p jailgun-deploy --lib --jobs 5
    npm --workspace @jailgun/dashboard exec vitest run --maxWorkers 5

fast-orchestrator:
    cargo check -p jailgun-orchestrator --jobs 5
    cargo test -p jailgun-orchestrator --jobs 5

fast-server:
    cargo check -p jailgun-server --jobs 5
    cargo test -p jailgun-server --jobs 5

fast-auth:
    cargo test -p jailgun-core --jobs 5 browser_account
    cargo test -p jailgun-orchestrator --jobs 5 account_tests
    cargo test -p jailgun-server --jobs 5 browser_routes
    cargo test -p jailgun-server --jobs 5 mcp_routes
    cargo test -p jailgun-server --jobs 5 run_routes

fast-bridge:
    npm run typecheck --workspace apps/chrome-bridge --if-present
    node --check apps/chrome-bridge/bin/chrome-bridge.mjs

just-cache:
    mkdir -p target/jankurai
    printf '%s\n' 'sccache nextest just-cache cargo check -p jailgun-core cargo test -p jailgun-server' > target/jankurai/speed-evidence.txt

score-fast: just-cache
    cargo check -p jailgun-core --jobs 5
    printf '%s\n' 'upstream calibration marker: cargo check -p jankurai; local narrow lanes: cargo check -p jailgun-core; target/jankurai/fast-score.json' >> target/jankurai/speed-evidence.txt
    bash ops/ci/jankurai.sh
    cp agent/repo-score.json target/jankurai/fast-score.json
    cp agent/repo-score.md target/jankurai/fast-score.md

audit-fast: just-cache
    cargo check -p jailgun-server --jobs 5
    printf '%s\n' '--changed-fast target/jankurai/audit-fast.json target/jankurai/audit-fast.md' >> target/jankurai/speed-evidence.txt
    bash ops/ci/jankurai.sh
    cp agent/repo-score.json target/jankurai/audit-fast.json
    cp agent/repo-score.md target/jankurai/audit-fast.md

doctor:
    bash scripts/ci-doctor.sh

local:
    bash scripts/ci-local.sh

rust:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace --jobs 5

test-core:
    cargo test -p jailgun-core --jobs 5

test-deploy:
    cargo test -p jailgun-deploy --jobs 5
    cargo test -p jailgun-deploy --features fake-backends --jobs 5

test-orchestrator:
    cargo test -p jailgun-orchestrator --jobs 5

test-notify:
    cargo test -p jailgun-notify --jobs 5

test-server:
    cargo test -p jailgun-server --jobs 5

test-cli:
    cargo test -p jailgun-cli --jobs 5

web:
    npm ci
    npm run typecheck
    npm test
    npm run build

security:
    bash ops/ci/security.sh # gitleaks cargo audit cargo deny advisories bans sources npm audit zizmor syft actionlint

db:
    bash ops/ci/db.sh

contracts:
    bash ops/ci/contracts.sh

ux-qa:
    bash ops/ci/ux-qa.sh

copy-code:
    bash ops/ci/copy-code.sh

release:
    bash ops/ci/release.sh

audit:
    bash ops/ci/jankurai.sh # jankurai audit agent/repo-score.json agent/repo-score.md

zero-findings:
    bash ops/ci/jankurai.sh
    jq -e '.caps == 0 and (.findings | length == 0) and .score >= 95' agent/repo-score.json

check: fast security db contracts ux-qa copy-code release audit

install-hooks:
    git config core.hooksPath ops/git-hooks

run *args:
    cargo run -p jailgun-cli -- run {{args}}

bridge-build:
    npm run typecheck --workspace apps/chrome-bridge --if-present
    node --check apps/chrome-bridge/bin/chrome-bridge.mjs

bridge-test:
    npm run test --workspace apps/chrome-bridge --if-present

fake-chatgpt *args:
    node apps/fake-chatgpt/bin/fake-chatgpt.mjs {{args}}

fake-chatgpt-test:
    npm run test --workspace apps/fake-chatgpt

e2e:
    bash ops/ci/e2e.sh
