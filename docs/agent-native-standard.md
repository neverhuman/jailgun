# Agent-Native Standard

This repository adopts the Jankurai agent-native pattern in advisory mode.
The local source of truth is `agent/JANKURAI_STANDARD.md`, with machine-readable
ownership in `agent/owner-map.json` and validation routing in
`agent/test-map.json`.

The practical rule for Jailgun is simple: policy and durable behavior belong in
Rust, the dashboard belongs in React, and runtime state belongs outside Git.

