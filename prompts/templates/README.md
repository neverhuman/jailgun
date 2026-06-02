# Prompt Templates

Prompt templates are parameterized instruction files that tell an AI
agent what to do. They live in `prompts/templates/` as plain-text files
with `{{PLACEHOLDER}}` markers where task-specific values are
substituted before the prompt is sent to an agent.

## How templates work

A template is just a `.txt` file containing agent instructions with
holes. The holes are filled — either by a human or by JMCP — to produce
a concrete prompt that gets delivered to an agent (jekko, jailgun batch,
etc.).

```
template + parameters → filled prompt → agent execution
```

### Placeholders

Placeholders use double-brace syntax: `{{PARAM_NAME}}`. They are
case-sensitive and must match exactly. Every placeholder in a template
must be filled before the prompt is usable.

## Filling parameters

### Manually

Open the template, copy it, and replace each `{{...}}` with your
value. Paste the result into your agent's prompt input.

### Via JMCP

JMCP delivers a **task envelope** (a JPCM/2.0 JSON file) whose
`payload` contains the parameter values and a `template` field naming
the template to fill. The flow:

1. JMCP writes a task envelope into the jailgun inbox.
2. Jailgun reads the envelope, loads the named template from
   `prompts/templates/`, and substitutes `payload` fields into
   placeholders.
3. The filled prompt is delivered to the agent (jekko) for execution.
4. The agent runs the task, respecting CI gates and commit rules from
   the template.

See `contracts/fixtures/jmcp/harden-task.json` for a concrete envelope
example.

## Available templates

| Template | Placeholders | Purpose |
|---|---|---|
| `harden-repo.txt` | `REPO_NAME`, `REPO_PATH`, `CI_COMMAND`, `FOCUS_AREA` | Targeted codebase hardening |

## Examples

### Filling harden-repo.txt for jailgun

```
REPO_NAME   = jailgun
REPO_PATH   = ~/code/jailgun
CI_COMMAND  = bash ops/ci/rust.sh && bash ops/ci/node.sh
FOCUS_AREA  = error handling
```

The filled prompt begins:

> You are a codebase hardening agent. Your job is to improve the
> reliability, safety, and correctness of jailgun by making targeted
> improvements in the area of **error handling**.

### Filling harden-repo.txt for a different repo

```
REPO_NAME   = jmcp
REPO_PATH   = ~/code/jmcp
CI_COMMAND  = npm test && npm run lint
FOCUS_AREA  = input validation
```

The filled prompt begins:

> You are a codebase hardening agent. Your job is to improve the
> reliability, safety, and correctness of jmcp by making targeted
> improvements in the area of **input validation**.

## Future: JMCP task envelopes

Today, template filling is either manual or wired into jailgun's
envelope handler. As JMCP matures:

- **Envelope-driven scheduling**: JMCP cron or inbox triggers will
  fire harden-task envelopes on a schedule, producing recurring
  hardening runs without human intervention.
- **Lease authority**: Task envelopes will carry real JMCP leases
  (replacing the current `jmcp-v0-pre-lease` sentinel) so agents can
  verify they are authorized before making writes.
- **Result envelopes**: Agents will write result envelopes back to
  JMCP's inbox summarizing what was hardened, enabling audit trails and
  notification chains.

See `docs/jmcp-integration.md` for the full integration architecture.
