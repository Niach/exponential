---
title: Code guides
description: Use Code's pure core from a checkout, write a pipeline file, and change the spec.
---

## Use it today

Code has no release tag yet. Take the pure core from a checkout, with a git dependency on `apps/desktop/crates/ecode-core` at a pinned `rev`; after the first `ecode-v*` tag, the rules of [Install from a release tag](/docs/sdk/install/) apply.

```rust
use ecode_core::{glob_match, validate_pipeline};

let file: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(".exponential/ci.json")?)?;
let errors = validate_pipeline(&file); // every error with its JSON pointer
assert!(glob_match("apps/web/**", "apps/web/src/a.ts"));
```

## A pipeline file

```json
{
  "version": 1,
  "name": "gate",
  "on": { "pull_request": { "branches": ["main"] } },
  "jobs": {
    "typecheck": { "runsOn": ["linux"], "steps": [{ "name": "Typecheck", "run": "bun run typecheck" }] },
    "web": {
      "name": "web tests",
      "runsOn": ["linux"],
      "needs": ["typecheck"],
      "paths": ["apps/web/**", "packages/**"],
      "matrix": { "shard": ["1", "2"] },
      "steps": [{ "name": "Test", "run": "bun run test --shard $MATRIX_SHARD/2" }]
    }
  },
  "required": ["typecheck", "web tests (1)", "web tests (2)"]
}
```

Each job becomes one check per matrix value (`web tests (1)`). A job runs only on a runner that has every label in its `runsOn`. A PR from a fork never runs.

## Change the spec

The contract and the fixtures are the one source. Edit `packages/ecode/contract.json` and regenerate the four languages:

```sh
bun run --filter @exp/ecode generate
```

After a deliberate change to a rule, recompute the fixtures from the TypeScript reference and review the diff:

```sh
bun run --filter @exp/ecode bless
```

The full contract and crate: [Reference](/docs/sdk/code/reference/).
