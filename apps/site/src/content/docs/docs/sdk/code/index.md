---
title: Code
description: Git and CI as one library. Forge providers (GitHub or our own), PR trees, a pipeline file and runners on your team's devices, with or without a server.
---

Code is git and CI in one library. One set of traits covers where objects go (`Remote`), the review unit (`Forge`) and pipelines (`Ci`). GitHub is one provider; Code's own is the other, and it runs with the Exponential server or with no server at all. Apache-2.0.

## Status

The spec is built: one contract plus fixtures, replayed in TypeScript and Rust. The providers come next.

| Part | Where | State |
|---|---|---|
| **Contract + fixtures** | `@exp/ecode` (private): forge, checks, pipeline, runner, remote | built |
| **Pure core** | `ecode-core` (Rust; serde only, no I/O) | built |
| **Swift / Kotlin** | generated into ExpCore (`ECode`) and the Android app (`com.exponential.app.ecode`) | built |
| **Server seam** | GitHub behind one `Forge` interface in Exponential | planned (#EXP-1614) |
| **Device crate** | `ecode`: the traits, local repos on gix, the providers | planned (#EXP-1615) |
| **Own git** | refs on the host, packs in the team bucket, `git-remote-exp` | planned (#EXP-1616) |
| **Own CI** | `.exponential/ci.json`, a runner in the daemon, logs in the bucket | planned (#EXP-1617) |
| **Release** | the `ecode-v*` tag | planned |

## What the spec covers

| Fixture | What |
|---|---|
| Forge | Open a PR on any base, list by base, retarget, mergeability, squash merge (the children move onto the base), ordered multi-merge, close, files, `rebase --onto`, push |
| Checks | The rollup and the merge gate over the required checks |
| Pipeline | `.exponential/ci.json`: validation, globs, and the planner (triggers, `needs`, matrix, `paths`, forks never run) |
| Runner | Claim by labels and slots, timeouts, lost runners, jobs queued too long, cancel |
| Remote | Remote URLs, the credential-helper scope, token refresh, log redaction |

Next: [Guides](/docs/sdk/code/guides/) · [Reference](/docs/sdk/code/reference/)
