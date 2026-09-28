# VAPP-3 Lane B: the Rust peer core on iOS

Throwaway. PeerHello = a one-screen SwiftUI app that drives `PeerLink` (UniFFI, `rust/peer-ffi`)
against the spike daemon through the steer relay; plus the size and cold-start measurements on the
REAL Exponential app (`apps/ios`, gated, see below).

## Build the xcframework

```bash
spike/vapp-3/ios/build-xcframework.sh                               # full,apple-crypto (default)
spike/vapp-3/ios/build-xcframework.sh --features full,rust-crypto --no-xcframework
spike/vapp-3/ios/build-xcframework.sh --features minimal,apple-crypto --no-xcframework
```

Per variant: `cargo rustc --crate-type staticlib` for `aarch64-apple-ios` + `aarch64-apple-ios-sim`
(peer-ffi also declares `cdylib`+`lib`, and cargo skips LTO when an rlib is among the crate types:
overriding to staticlib alone is what gets the fat-LTO build), Swift bindings from a host cdylib
(`uniffi-bindgen generate --library libpeer_ffi.dylib`, run from `spike/vapp-3/rust`: it needs
`cargo metadata`), `out/PeerFFI.xcframework` (headers dir = `peer_ffiFFI.h` + `module.modulemap`),
bindings copied to `PeerHello/Sources/Generated/`, `DevTickets.swift` written from
`results/.tickets-dev.json` + the Mac's en0 IP (`write-dev-tickets.sh`). Appends two `size` lines to
`results/b-size.jsonl` (`--no-results` skips): the `.a`, and the dead-stripped linked delta (a C main
that references every exported uniffi symbol, linked through `swiftc` because apple-crypto brings
Swift objects, stripped, minus an empty main). Everything in `out/` is a build output (gitignored).

## PeerHello

```bash
cd spike/vapp-3/ios/PeerHello && tuist generate --no-open
# simulator
xcodebuild -workspace PeerHello.xcworkspace -scheme PeerHello \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro' -derivedDataPath /tmp/vapp3-dd-peerhello build
xcrun simctl install booted /tmp/vapp3-dd-peerhello/Build/Products/Debug-iphonesimulator/PeerHello.app
xcrun simctl launch --console-pty booted at.exponential.peerhello -auto 1 [-autoReconnect 1] [-policy relay]
# device (signed, team V6W7BVCSM8, wildcard dev profile)
xcodebuild -workspace PeerHello.xcworkspace -scheme PeerHello -configuration Release \
  -destination 'generic/platform=iOS' -derivedDataPath /tmp/vapp3-dd-peerhello -allowProvisioningUpdates build
xcrun devicectl device install app --device 00008150-00161C411130401C \
  /tmp/vapp3-dd-peerhello/Build/Products/Release-iphoneos/PeerHello.app
xcrun devicectl device process launch --device 00008150-00161C411130401C at.exponential.peerhello
```

Launch arguments (UserDefaults): `-auto 1` runs Connect → Bench → Send once and prints
`PEERHELLO_RESULT <json>` + `PEERHELLO_DONE`; `-autoReconnect 1` adds an ICE restart before Send;
`-policy all|relay|host`, `-scenario …`, `-relay ws://…`, `-ticket …`, `-session …`, `-turn host:port`.

Buttons: **Connect** (WS join as viewer, waits for the replay end + the last daemon `hello`, creates
the link with `expectedRemotePubkey` = the hello key, sends the offer, pumps `pollSignals(200)` on a
thread and forwards every envelope with `to` = the daemon), **Bench** (`runBench(2_000_000, 100)`),
**Reconnect** (`restartIce`; the new offer leaves through the pump; time = restart → an answer after
it + state Connected), **Send results** (README bench line as a `result` envelope through the relay,
and POSTed to `http://<mac>:8787/results` when reachable), **Copy JSON**, **Close**.
Automatic reconnects: scene becomes active after being backgrounded (`trigger: background`) and an
`NWPathMonitor` interface change (`trigger: network-switch`); the WS is reopened first when it died.

## Real-app size + cold start (apps/ios, gated)

`TUIST_PEER_SPIKE=1 tuist generate` (tuist forwards only `TUIST_`-prefixed env to manifests; a bare
`PEER_SPIKE=1` is invisible) adds to ExpCore: the xcframework, CryptoKit/SystemConfiguration/libresolv,
`out/bindings/peer_ffi.swift` as a source and the `PEER_SPIKE` compilation condition, which turns on
`ExpCore/Sources/Spike/PeerSpike.swift` (public, so the dylib exports it and the linker keeps the core).
Without the env var the project generates exactly as before.

```bash
cd apps/ios
tuist generate --no-open                    # baseline   (TUIST_PEER_SPIKE=1 for with-core)
xcodebuild -workspace Exponential.xcworkspace -scheme Exponential -configuration Release \
  -destination 'generic/platform=iOS' -derivedDataPath /tmp/vapp3-dd-app<variant> \
  CODE_SIGNING_ALLOWED=NO ARCHS=arm64 ONLY_ACTIVE_ARCH=YES build
spike/vapp-3/ios/size-ios.sh /tmp/vapp3-dd-app<variant> baseline|full-apple-crypto
spike/vapp-3/ios/coldstart-ios.sh baseline|with-core ['id=<device udid>']
```

`size-ios.sh` records the unstripped `build` output AND an archive-style stripped copy (`strip -S -x`
on every Mach-O; `xcodebuild build` does not strip, an archive does) plus a zip as the download proxy.
`coldstart-ios.sh` runs `ExponentialUITests/LaunchMetricTests` (XCTApplicationLaunchMetric ×5,
Release), which skips unless the runner env has `VAPP3_LAUNCH_METRIC=1` (xcodebuild:
`TEST_RUNNER_VAPP3_LAUNCH_METRIC=1`, set by the script). Re-run `tuist generate --no-open` afterwards
(the with-core project stays generated otherwise).

## Danny: the matrix on the iPhone Air

1. Unlock the iPhone (Developer Mode on; the Mac pairing is over Wi-Fi), then from the repo root:
   `spike/vapp-3/ios/build-xcframework.sh` and the device build/install/launch commands above.
   First launch: Settings → General → VPN & Device Management → trust the developer profile if asked;
   allow "Local Network" when prompted (PeerHello needs it for the LAN relay/TURN).
2. The Mac side must be up: coturn (`scripts/coturn-up.sh`), the docker relay, the daemon
   (top-level README step 3). The relay/TURN defaults point at the Mac's LAN IP (192.168.178.71).
   **Off the home LAN** (cellular, office Wi-Fi) that IP is unreachable: type the Mac's public
   host/port for Relay URL (`ws://<public>:4002`) and TURN (`<public>:3478`) with the router
   forwarding TCP 4002 + UDP 3478 and UDP 49160-49200 to the Mac, or run relay/coturn on a public box.
3. Per scenario (`same-lan`, `home-wifi-cellular` = phone on cellular / Mac on home Wi-Fi,
   `home-office-wifi`, `cellular-cellular` = Mac tethered to a second phone): pick Scenario, Policy
   `all`, tap Connect (expect "connected in N ms" within a few seconds), Bench (2 MB up/down + 100
   pings, a few seconds on Wi-Fi, longer on cellular), Send results. Then Policy `relay` and repeat.
   Expect `localPath/remotePath` host↔host on the LAN, srflx/relay across NATs, relay on `relay`.
4. Reconnect: with a connected link, background the app for ~10 s and return (records
   `background`), and toggle Wi-Fi off/on or walk out of Wi-Fi range (records `network-switch`);
   tap Send results after each. The log section shows every step; Copy JSON if Send fails.
5. Cold start on the device (optional): `spike/vapp-3/ios/coldstart-ios.sh baseline 'id=00008150-00161C411130401C'`
   and `with-core` (Release, signed automatically; the phone must stay unlocked).

## Gotchas

- Xcode 27.0 beta is the active toolchain. Apple `nm` cannot read rustc 1.96's embedded LLVM-22
  bitcode ("Unknown attribute kind (105)"): read symbols from the generated header instead. Linking
  is unaffected (ld64 uses the machine code).
- apple-crypto (`apple-cryptokit-rs`) puts Swift objects into the `.a`: a pure C link fails on
  `__swift_FORCE_LOAD_$_swiftCompatibility51`; link through `swiftc` (Xcode targets do this anyway).
  Needs `CryptoKit`, `SystemConfiguration` (tokio/if-addrs) and `libresolv`.
- UniFFI 0.32 bindings are Sendable-clean: they compile warning-free inside ExpCore under Swift 6 +
  `SWIFT_STRICT_CONCURRENCY=complete`. PeerHello still uses Swift 5 mode for its own glue.
- `pollSignals`/`runBench`/`recv` BLOCK: never call them on the main actor. PeerHello runs the pump on
  a dedicated thread and the bench on a global queue; `acceptAnswer`/candidates go through a serial
  queue so they are not stuck behind a running bench.
- The relay replays the room log on join (many old `hello`s): wait for `activity_synced` before
  offering, and filter envelopes by `to`.
- The device build needs the phone UNLOCKED (xcodebuild: "needs to be unlocked to enable development
  services"; devicectl: "developer disk image could not be mounted … device was still locked").
