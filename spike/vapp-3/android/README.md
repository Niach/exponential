# VAPP-3 Lane C: Android (Rust peer core via UniFFI + JNA)

Throwaway spike. See `../README.md` for the envelope, relay transport and results line formats.

## Files

| Path | What |
|---|---|
| `build-so.sh` | `cargo ndk` → `out/jniLibs/<abi>/libpeer_ffi.so` (arm64-v8a, armeabi-v7a, x86_64, API 26), size lines, Kotlin bindings → `out/kotlin`, copies both + `DevTickets.kt` into PeerHello |
| `uniffi-kotlin.toml` | bindgen config: renames `PeerLink.close` → `closeLink` (clashes with `AutoCloseable.close`) |
| `write-dev-tickets.sh` | `results/.tickets-dev.json` → `PeerHello/.../DevTickets.kt` (gitignored) |
| `PeerHello/` | one-Activity Gradle app `at.exponential.peerhello` (plain Views, OkHttp WS, JNA, coroutines) |
| `fake-daemon.ts` | relay smoke publisher (hello + logs inputs), NOT the real daemon |
| `size-android.sh` | real-app APK delta (baseline vs `-PpeerSpike=true`) → `results/c-size.jsonl` |
| `coldstart-android.sh` | real-app cold start (`am start -W` TotalTime) → `results/c-coldstart.jsonl` |
| `real-app/proguard-peer-spike.pro` | R8 keep rules for JNA + `uniffi.peer_ffi` (spike builds only) |

## Build the .so + bindings

```bash
spike/vapp-3/android/build-so.sh                                  # full,rust-crypto (default)
spike/vapp-3/android/build-so.sh --features minimal,rust-crypto   # size variant
```

Uses `CARGO_TARGET_DIR=spike/vapp-3/rust/target` and NDK 28.2.13676358. Sizes are only recorded
once `peer-core` is real (a stub core is LTO'd to nothing). Pass `--no-record` to skip.

## PeerHello

```bash
export JAVA_HOME=$(/usr/libexec/java_home)
cd spike/vapp-3/android/PeerHello && ./gradlew :app:assembleDebug
ADB=~/Library/Android/sdk/platform-tools/adb
$ADB install -r app/build/outputs/apk/debug/app-debug.apk
```

Emulator, headless:

```bash
~/Library/Android/sdk/emulator/emulator -avd Medium_Phone_API_36.0 -no-window -no-audio -no-boot-anim &
$ADB wait-for-device; until [ "$($ADB shell getprop sys.boot_completed | tr -d '\r')" = 1 ]; do timeout 2 tail -f /dev/null; done
$ADB shell am start -n at.exponential.peerhello/.MainActivity --es auto bench --es policy all --es scenario emulator-nat [--es runs 5]
$ADB logcat -s VAPP3:I          # AUTO_LINE <bench json> per run, then AUTO_DONE
```

Extras: `auto` (`bench`), `policy` (all|relay|host), `scenario`, `runs`, `relay`, `ticket`,
`session`, `turn`. Every run: Connect → Bench (2 MB, 100 pings) → Send results (a `result`
envelope via the relay to the daemon; only if that fails, a POST to `http://<mac>:8787/results`).
A failed run still sends an `ok:false` line.

Buttons: **Connect** (relay join → last daemon `hello` → offer → trickle → answer → Connected),
**Bench**, **Reconnect** (ICE restart, time to Connected; also automatic after background →
foreground = `background` and on a default-network change = `network-switch`; the next bench
line carries `reconnect`), **Send results**, **Copy JSON**.

Relay smoke without the daemon: `bun spike/vapp-3/android/fake-daemon.ts --seconds 90`, then the
auto command above: the log shows `daemon hello from=fake-daemon` and the fake daemon prints the
offer as `INPUT {...}` (the connect then times out in Connecting, expected).

## Real app (apps/android), gated by `-PpeerSpike=true`

Without the property the build is unchanged except `BuildConfig.PEER_SPIKE=false` and a no-op
`PeerSpike.touch()` in `ExponentialApp.onCreate`. With it: `out/jniLibs` + `out/kotlin` source
dirs, JNA `@aar`, `proguard-peer-spike.pro`, debug signing when `RELEASE_STORE_FILE` is unset, and
`PeerSpike.touch()` reflectively calls `uniffi.peer_ffi.Peer_ffiKt.peerVersion()` (logs
`VAPP3 peer-ffi ... loaded in N ms`).

```bash
cd apps/android && export JAVA_HOME=$(/usr/libexec/java_home)
./gradlew :app:assembleProductionRelease > /tmp/vapp3-android-baseline.log 2>&1   # unsigned
cp app/build/outputs/apk/production/release/app-production-release-unsigned.apk ../../spike/vapp-3/android/out/apk/
~/Library/Android/sdk/build-tools/37.0.0/apksigner sign --ks ~/.android/debug.keystore --ks-pass pass:android \
  --ks-key-alias androiddebugkey --key-pass pass:android \
  --out ../../spike/vapp-3/android/out/apk/app-production-release-baseline.apk ../../spike/vapp-3/android/out/apk/app-production-release-unsigned.apk
rm -f ../../spike/vapp-3/android/out/apk/*.idsig    # else adb does an INCREMENTAL install (slow cold starts)
./gradlew :app:assembleProductionRelease -PpeerSpike=true > /tmp/vapp3-android-peerspike.log 2>&1
cp app/build/outputs/apk/production/release/app-production-release.apk ../../spike/vapp-3/android/out/apk/app-production-release-peerspike.apk
cd ../../spike/vapp-3/android
./size-android.sh out/apk/app-production-release-baseline.apk out/apk/app-production-release-peerspike.apk
./coldstart-android.sh out/apk/app-production-release-baseline.apk baseline 15
./coldstart-android.sh out/apk/app-production-release-peerspike.apk with-core 15
```

## Danny: the phone matrix

Prereqs on the Mac: coturn up, relay up, daemon running (see `../README.md`), `build-so.sh` run
after the tickets were minted (the viewer ticket is baked into `DevTickets.kt`), PeerHello built.

1. Plug the phone in (USB debugging on), `adb install -r PeerHello/app/build/outputs/apk/debug/app-debug.apk`.
   (No cable: AirDrop/upload the APK and allow "install unknown apps".)
2. Open PeerHello. Check: Relay URL `ws://192.168.178.71:4002`, TURN `192.168.178.71:3478`.
   Off the home LAN the phone cannot reach those private IPs: the relay and coturn must be
   reachable from the internet (port-forward 4002/tcp, 3478/udp+tcp, 49160-49200/udp to the Mac, or
   a public host), then type the public host into both fields.
3. Per scenario pick **Scenario** + **Policy** and tap **Connect** → wait for `state=CONNECTED in N ms`
   → **Bench** → **Send results**. Do each scenario with policy `all` and `relay`:
   - `same-lan`: phone on home Wi-Fi.
   - `home-wifi-cellular`: phone on cellular (Wi-Fi off), Mac on home Wi-Fi.
   - `home-office-wifi`: phone on another Wi-Fi (office/café).
   - `cellular-cellular`: needs the daemon on a second phone's hotspot/cellular (skip if not possible).
4. Reconnect: after a Connected + Bench, press Home for ~10 s, reopen PeerHello (auto `background`
   reconnect, status shows `reconnect(background): CONNECTED in N ms`), tap Bench + Send. Then toggle
   Wi-Fi off while connected (auto `network-switch`), tap Bench + Send.
5. If Send fails (no relay), tap **Copy JSON** and paste the line into the VAPP-3 issue.

Expected: Connected within a few seconds, `localPath/remotePath` = host on LAN, srflx/prflx across
NATs, `relay` with policy relay. A reject/fingerprint error = the tamper check fired (bad ticket or
wrong daemon key).

## Gotchas

- **JNA**: UniFFI Kotlin needs `net.java.dev.jna:jna:<v>@aar` (the jar has no Android natives). The
  aar ships `libjnidispatch.so` for 7 ABIs incl. armeabi/mips/mips64/x86 (dead weight in a universal
  APK; add `abiFilters` or rely on bundle splits). R8 needs `-keep class com.sun.jna.** { *; }` +
  `-keep class uniffi.<crate>.** { *; }` + `-dontwarn java.awt.**`.
- **Bindings package** = `uniffi.peer_ffi`; top-level functions live in `uniffi.peer_ffi.Peer_ffiKt`.
  u32/u64 map to `UInt`/`ULong`. Errors are `PeerException` (UniFFI renames `*Error`).
- **Exported `close()`** clashes with `AutoCloseable.close()` ("Conflicting overloads"): fixed by
  the rename in `uniffi-kotlin.toml` (UniFFI 0.32 `--config` expects `[crates.<name>.bindings.kotlin]`).
- **Stripped .so has no UniFFI metadata**: `uniffi-bindgen --library` on the cargo-ndk output fails
  ("No UniFFI metadata found"); generate from the host `libpeer_ffi.dylib` (same metadata).
- **16 KB pages**: 64-bit `.so` LOAD alignment = `0x4000` (cargo-ndk 4 / NDK r28 default); armv7 is
  `0x1000` (32-bit is exempt). `zipalign -c -P 16 4` passes on the APK (AGP stores `.so` uncompressed).
- **Cleartext**: `android:usesCleartextTraffic="true"` for `ws://` and the `http://…:8787` POST.
- **Emulator NAT**: the Mac is `10.0.2.2`; the emulator's own host candidates are `10.0.2.15/16`
  (unreachable from the Mac), its srflx is the Mac's NAT address; the `emulator-nat` scenario is a
  double-NAT approximation, not a phone.
- **Cold start**: the real app requests POST_NOTIFICATIONS on first launch; the dialog becomes the
  launched activity and `am start -W` prints no TotalTime, so the script pre-grants it.
