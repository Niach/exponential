---
title: eAgent guides
description: Depend on eagent and run the user's own Claude Code or Codex from your app with eagent::local.
---

## Install

A git dependency on a release tag: the contract is compiled in from the repo's fixtures. The policy: [Install from a release tag](/docs/libraries/install/).

```toml
[dependencies]
eagent = { git = "https://github.com/Niach/exponential", tag = "eagent-v0.1.0" }
```

Coming from `exponential-agent-runtime`: [Migrating](/docs/libraries/eagent/reference/migrating/).

## Run an agent without a server

`eagent::local` is the whole surface a single-account host needs:

```rust
use eagent::{agent::CodingAgent, local::{LocalAgent, RunRequest}};

let agent = LocalAgent::new(CodingAgent::Claude);    // or ::with_program(agent, "/abs/claude")
agent.check();                                       // `<cli> --version` -> ToolCheck
agent.signed_in();                                   // the CLI's own auth status: Option<bool>
agent.login(false)?;                                 // LocalLogin: the CLI's own sign-in in a PTY
let run = agent.start(
    RunRequest::new(cwd)
        .prompt("…")                                 // the first message
        .resume(past.id)                             // or reopen a past conversation
        .mcp_server(server).env("NOTES_TOKEN", tok)  // extra servers, `${VAR}` refs into env
        .system_append("…"),
)?;
run.events();                                        // flume::Receiver<wire::ActivityEvent>
run.send("…");  run.answer(question_id, keys, text);  run.interrupt();  run.stop();
agent.sessions(Some(&cwd), Some(20));                // Vec<PastSession>, newest first
```

Try it from `apps/desktop` in the repo:

```sh
cargo run -p eagent --example local_chat -- "reply with the word ok"
cargo run -p eagent --example local_chat -- --agent codex "hi"
```

Events, history and the seams in full: [Reference](/docs/libraries/eagent/reference/).
