# Concept workflows

A run records its input, constraints, evaluation criteria, model preference and
template version before it starts. Every interface uses the same Rust workflow
service. Text workflows do not require a repository or source archive and never
authorize deployment.

## Stages and perspectives

1. **Explore:** generate five to ten distinct candidates with the same concept,
   constraints and criteria.
2. **Compare:** evaluate the completed candidates and retain scores, rationales,
   disagreements and uncertainty.
3. **Synthesize:** combine compatible strengths and explain the choices.
4. **Critique:** identify weak assumptions, risks and failure cases.
5. **Revise:** produce the final proposal, alternatives, tradeoffs, unresolved
   assumptions and next steps.

The first five perspectives are the default. Higher candidate counts append
perspectives in this stable order:

| # | Perspective |
| --- | --- |
| 1 | User needs and practical value |
| 2 | Novel approaches and differentiation |
| 3 | Technical feasibility and implementation |
| 4 | Constraints, cost and operational simplicity |
| 5 | Adversarial critique and failure prevention |
| 6 | Adoption and distribution |
| 7 | Scalability and long-term operation |
| 8 | Evidence and testable assumptions |
| 9 | Alternative architectures or business models |
| 10 | Integration of competing approaches |

Candidate text is supplied as data to subsequent stages. It cannot change the
workflow, authorize tools or alter the account's settings. Full responses are
retained. Comparison uses validated structured summaries of at most 6,000
characters each; each stage input must fit 128,000 characters. An oversized
input returns `context-too-large`, rather than silently truncating evidence.

## Evaluation

| Criterion | Default weight |
| --- | --- |
| Usefulness | 30% |
| Feasibility | 25% |
| Novelty | 20% |
| Supporting evidence | 15% |
| Risk management | 10% |

Before submission, supply one to ten uniquely named criteria with positive
integer weights totaling 100. Each candidate receives a model-generated score
from 0 to 100 for each criterion; the weighted score orders the comparison.
These judgments are not independently established facts. The final result
retains the criteria and model preference used for the run; each captured
attempt records its observed model and conversation reference.

## Execution limits

| Setting | Default |
| --- | --- |
| Candidate count | 5, selectable from 5–10 |
| New account capacity | 10 active conversations |
| Account-wide spacing | 30 seconds plus up to 5 seconds of jitter |
| Response timeout | 30 minutes after acceptance |
| Run deadline | 3 hours from creation |
| Automatic attempts | At most 2 per candidate or reduction stage |
| Normal submissions | Candidate count + 4 |
| Hard submission budget | Twice the normal submission count |

Comparison, synthesis, critique and revision share the account's capacity and
pacing with every other run. Provider cooldowns pause new submissions across
the account. A known retry time is retained; otherwise the first recheck waits
five minutes. Repeated limits require operator attention. Closing a provider
dialog does not restore capacity.

Automatic retry requires a definitely unaccepted submission or an explicitly
retryable completed attempt. Restart does not reset counts, cooldowns or
deadlines. Pausing prevents new submissions while accepted work can still be
captured; the overall deadline continues to apply.

## Partial results and interruption

By default every requested candidate must complete. If bounded retries cannot
finish exploration, choose **Retry**, **Continue with completed candidates**, or
**Cancel**. Continuing requires an explicit action, at least three complete
candidates, and no active exploration attempts. Excluded candidates are listed
in the final result. Interrupted text stays marked `partial` with its original
error, even when a file was saved successfully.

Cancellation is persistent and idempotent and retains captured work. The
supervisor stops only conversations owned by the cancelled run. On restart it
can reattach a known accepted turn. A submission whose acceptance is uncertain
requires reconciliation; it is never automatically repeated. Reconnect an
expired account in the dashboard to retain completed work and resume eligible
work without a new run.

## Outputs

The result manifest links full candidate Markdown, structured candidate
summaries, comparison and ranking, initial synthesis, critique and revised
`final.md`. Files include integrity metadata and complete/partial status.
Read them in Results or export with `jailgun runs export RUN_ID --out DIRECTORY`.
Completed results remain available after daemon restart. See
[CLI operations](cli.md), [dashboard controls](dashboard.md) and
[backup and restore](../db/README.md).
