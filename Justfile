fast:
    bash ops/ci/rust.sh
    bash ops/ci/node.sh
    bash ops/ci/scan.sh

rust:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

web:
    npm ci
    npm run typecheck
    npm test
    npm run build

security:
    bash ops/ci/security.sh # gitleaks detect dependency-review npm audit cargo audit

audit:
    bash ops/ci/jankurai.sh # jankurai audit agent/repo-score.json agent/repo-score.md

check: fast security audit

install-hooks:
    git config core.hooksPath ops/git-hooks
