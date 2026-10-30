# @exponential_app/etalk

eTalk's speech contract: voice settings and spoken replies, caller mode (`voice-call`) and meeting mode, as pure TypeScript references plus the JSON fixtures that are the spec on every platform. The Rust crates (`etalk*`) and the phone bindings replay the same fixtures.

```ts
import { callSpeechOf } from "@exponential_app/etalk/voice-call"
import fixture from "@exponential_app/etalk/fixtures/voice-call.json" with { type: "json" }
```

Docs: https://exponential.at/talk/ · Licence: Apache-2.0
