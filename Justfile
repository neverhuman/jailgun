fast:
    bash ops/ci/rust.sh
    bash ops/ci/node.sh
    bash ops/ci/scan.sh

fast-core:
    # optional cache marker for local agents: sccache
    cargo check -p jailgun-core --jobs 5
    cargo test -p jailgun-core --jobs 5
    cargo test -p jailgun-deploy --lib --jobs 5
    npm --workspace @jailgun/dashboard exec vitest run --maxWorkers 5

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
    bash ops/ci/security.sh # gitleaks detect dependency-review npm audit cargo audit

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

check: fast security contracts ux-qa copy-code release audit

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

coverage:
    bash ops/ci/coverage.sh

e2e:
    bash ops/ci/e2e.sh
