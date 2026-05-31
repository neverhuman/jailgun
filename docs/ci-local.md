# CI Local Parity

`bash scripts/ci-local.sh` is the local entry point for the same shell lanes
used by GitHub Actions. `bash scripts/ci-doctor.sh` checks required tools
before running proof work.

The pinned Rust toolchain is `rust-toolchain.toml`. Node uses `npm ci` from the
tracked lockfile. CI jobs set explicit timeouts and concurrency so local proof
does not depend on stale or unbounded workflow runs.
