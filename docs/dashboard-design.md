# Dashboard design and visual review

The concept dashboard has four destinations: accounts, new concept, run progress
and results. Each screen puts the operator's next action first. The archive
monitor remains available through the advanced-workflow link.

`apps/dashboard/src/tokens.css` owns the shared green/neutral palette, type sizes,
spacing, control radii and status colors. Both dashboards consume these design
tokens. Keep complete, partial, excluded and failed labels visible; color alone
must not carry state. Native controls have explicit labels and visible focus.

During account verification, confirming the observed identity and model is the
primary action. The browser's current model is readable outside the select
control. Comparison tables retain complete scores and rationales at narrow
widths: their labeled scroll region is keyboard focusable, with a visible
scrolling hint. The shell keeps its footer below the workspace during loading.
An unavailable service gets an actionable error before account setup is offered.

Run `bash ops/ci/ux-qa.sh` after a build-affecting change. Review actual desktop
and mobile PNGs from `artifacts/ux-qa/`, alongside ARIA snapshots, axe results,
measured layout stability and the input hashes in `target/jankurai/ux-qa.json`.
The scripted checks cannot judge visual hierarchy or legibility by themselves.

For a visual review, record the reviewer, current time, screenshot paths and
SHA-256 hashes, the observations, and any unresolved findings under the private
`target/release-evidence/` directory. Re-render and inspect affected screens after
fixing a finding. Never copy a prior pass onto new images. Review the new-concept
form, account verification, partial progress, comparison and final concept at
both viewport sizes. Inspect actual content wrapping, primary-action hierarchy,
focus visibility, long model names and table scrolling. The Linux login viewer
also needs the separate real RFB interaction proof described in `testing.md`.

These fixtures use synthetic content. Screenshots used in public documentation
must identify that fact; they are not evidence of live-provider acceptance.
