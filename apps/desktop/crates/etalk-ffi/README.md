# `etalk-ffi`: eTalk for Swift and Kotlin

The UniFFI 0.31 facade over [`etalk`](../etalk) (the call machine) and
[`etalk-codec`](../etalk-codec) (Opus) for iOS, macOS and Android, so a phone
places the same call as the desktop (#EXP-1441): an xcframework / an Android
`.so` plus the generated bindings. gpui-free, engine-free, no app crate.

Audio I/O stays native (AVAudioEngine; Oboe / AudioRecord): the facade takes
and returns PCM frames. Transport is not eTalk's job: the packets ride eNet's
call channel (`exp/enet/call/1`, `exponential-net-ffi`'s `sendFrame`).

## The contract

The spec is `packages/etalk/fixtures/voice-call.json` (its `$comment`); the
TypeScript reference is `packages/etalk/src/voice-call.ts`.

- The call machine, pure functions over plain records:
  - `callIdle()` → `callStep(state, event)` → `callView(state, platform)`
    (None = no call surface). `TalkCallEvent` = `start {device, language}`,
    `rang`, `ready`, `noSound`, `partial`, `said`, `speechStarted`,
    `queued {count, text}`, `replyStarted`, `replyFinished`, `mute`, `unmute`,
    `speaker {on}`, `end`, `hungUp {reason?}`, `failed {code}`. A finished
    call with a failure leaves `state.toast`.
  - `callUnavailable(input)` (None = the telephone is enabled) and
    `callSpeechOf(doctor row state)`.
  - The ring (#EXP-1423): the HOST draws `callRingRandom()` once per call and
    steps `rang` at `callRingMs(random)`; `ready` = the voice device's warm-up
    done; `callAnswerMs(random, readyMs)` = when it answers (None = never:
    fail it `unavailable` at `maxRingMs`); then the agent speaks first (the
    greeting, `callGreeting(language)`). `callRingToneOn(ms)` = the ring
    tone's cadence (`ringLowHz` + `ringHighHz`, looping until answered).
  - `callTimer(ms since answered)`.
  - The wire form of a flow event: `callEventOfWire(json)` for a caller's
    incoming `call_event`, `callWireEvent(event)` for a host's outgoing one.
  - `callConstants()` (the fixture's numbers + `wire`), `callSpecJson()` (the
    fixture itself: copy, icons, test ids).
- The codec (mono, `frameMs` = 20 ms frames, i16 PCM):
  - `TalkEncoder(rate, bitrate, lossPercent?)` → `encode(pcm)` → one packet
    (a caller: `upRate` at `upKbps`). `lossPercent` turns in-band FEC on.
  - `TalkDecoder(rate)` → `decode(packet)` (an empty packet conceals a lost
    frame), `decodeFec(next packet)` = the lost frame before it (a caller:
    `downRate`, behind its `jitterMs` buffer).
  - `TalkUplink(rate)` → `push(seq, packet)`: the host's receive path, one lost
    packet recovered from the next one's FEC.
  - `codecBackend()` = `opus-rs` (pure Rust, no C toolchain).
- Errors: `TalkError` = `Invalid | Codec`, each with a `reason`. A Rust panic
  never crosses the FFI: the machine is total and the codec turns an opus-rs
  panic into `Codec` with a fresh coder behind it.

## Build

```bash
bash apps/desktop/crates/etalk-ffi/generate-bindings.sh  # bindings/swift + bindings/kotlin from the host library
bash apps/desktop/crates/etalk-ffi/build-ios.sh          # out/ExponentialTalkFFI.xcframework (+ bindings)
bash apps/desktop/crates/etalk-ffi/build-android.sh      # out/jniLibs/<abi>/libetalk_ffi.so (+ bindings)
bash apps/desktop/crates/etalk-ffi/run-binding-tests.sh  # the Swift + Kotlin suites against the host dylib
```

Device builds use the workspace's `mobile` profile (LTO, one codegen unit,
stripped), one crate type per build so LTO applies. `build-ios.sh` takes a
lock per target dir (a shared `CARGO_TARGET_DIR` is safe). The generated
binding sources under `bindings/` are COMMITTED (the contract, reviewable
without a toolchain; Kotlin ships with `omit_checksums`, so regenerate after
every `lib.rs` change); `out/` is ignored. Kotlin uses JNA (≥ 5.12), like
`exponential-ui-ffi`.

`tests/facade.rs` replays the fixture through the facade's types from Rust;
`tests/swift/main.swift` and `tests/kotlin/FacadeTest.kt` replay the same
fixture through the bindings (the events built natively, and once more
through their wire form) and round-trip a 440 Hz tone through the codec with
one packet lost and recovered.

## Distribution

Every `etalk-v*` tag (`release-etalk.yml`, `docs/libraries.md`) publishes:

- **SwiftPM**: `https://github.com/Niach/exponential-talk-swift`, tagged
  `<version>`, product `ExponentialTalk` (`import ExponentialTalk`; iOS 15+,
  macOS 12+). The xcframework zip is an asset of the `etalk-v<version>`
  release. `release/swift-package.sh` writes the package (`--local` for a
  path binary you can `swift build`).
- **Maven Central**: `at.exponential:etalk:<version>` (an AAR: the `.so` for
  arm64-v8a, armeabi-v7a, x86_64 + the binding, package
  `at.exponential.talk.ffi`; JNA comes along). `android/` is the Gradle
  library on eUI Compose's toolchain; `release/central-bundle.sh` is the
  Portal bundle.

```swift
.package(url: "https://github.com/Niach/exponential-talk-swift", exact: "0.2.1")
```

```kotlin
implementation("at.exponential:etalk:0.2.1")
```
