# Ops Agent Instructions

This area owns local and GitHub CI orchestration:

- `ops/ci/**`
- `ops/git-hooks/**`
- `.github/workflows/**`

Do not edit product source, dashboard source, package manifests, or Cargo
workspace files from this scope. Keep CI commands reproducible locally through
`bash ops/ci/*.sh`; workflows should delegate to those scripts instead of
inlining long command sequences.

Required proof after CI or governance changes:

```bash
bash ops/ci/scan.sh
bash ops/ci/jankurai.sh
```

The Jankurai lane owns generated audit and badge artifacts. Do not hand-edit
`agent/repo-score.*` or `agent/jankurai-badge.*`; rerun the lane instead.
