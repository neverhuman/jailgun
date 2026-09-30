# Advanced deployment preservation

Concept workflows never invoke deployment. Archive deployment remains an
operator-authorized advanced workflow and requires a dedicated, quiescent
checkout. Stop editors, build watchers and independent Git automation that can
write to that checkout before deploying.

Cleanup rejects dirty checkouts and missing HEAD or `origin/main`. For a
divergent checkout using `preserve-reset`, it creates a new preservation ref,
writes and syncs a private receipt, fetches, and verifies that the checkout is
still clean and HEAD still matches the preserved commit. It records the fetched
target before reset. The remote reset operation then acquires
`<git-dir>/jailgun-mutation.lock` and rechecks HEAD, the preservation ref, target
and cleanliness immediately before reset. It does not run `git clean`.

The launcher holds the same mutation lock throughout extraction, the remote
command and failure preservation. A second Jailgun mutation fails with
`checkout-owned`. A lock left after a killed process is retained: inspect the
owning processes, checkout, preservation refs and job status before removing an
abandoned empty lock directory. Jailgun does not infer that a lock is stale from
its age. This cooperative lock coordinates Jailgun processes; it cannot prevent
an unrelated program from writing files directly.

If the remote command fails, reset requires all of the following:

- Any new commit has a verified preservation ref.
- Dirty work has a successful stash with a separate verified preservation ref.
- The preservation status is written and synced.
- HEAD and cleanliness still match the preserved state.

Disabling stashing leaves dirty work in place and fails. Ref, stash or receipt
failures also leave the checkout for inspection, without reset. A successful
preservation reset reports `failed-preserved`; it never turns a failed remote
command into a successful deployment.

Local cleanup receipt replacement is atomic and private. An interrupted
replacement leaves the prior receipt intact and a `.json.writing` file for
inspection. Retain the checkout, refs, job artifacts and receipts while resolving
a failure.

`cargo test --locked -p jailgun-deploy --all-features` executes the generated
shell against disposable local Git repositories, including paths with spaces,
failed preservation, failed receipt writes, mutation ownership, dirty checkout
changes and successful preservation. These checks require no remote credentials.
