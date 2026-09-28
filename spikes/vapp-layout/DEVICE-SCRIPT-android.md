# VAPP-4 · Android device script (real mid-range phone)

Goal: the kitchen-sink and bench-200 numbers on real hardware, a TalkBack
sweep, and a hand typing test. About 15 minutes. The phone must be arm64
(the spike `.so` ships `arm64-v8a` only).

## Setup

1. Enable Developer options + USB debugging on the phone, plug it in, accept
   the RSA prompt. `adb devices` must list it (and nothing else, or add
   `-s <serial>` to every `adb` line below).
2. Build + install the debug APK from the repo root:
   ```bash
   cd apps/android
   JAVA_HOME=$(/usr/libexec/java_home) ./gradlew :app:assembleProductionDebug
   adb install -r app/build/outputs/apk/production/debug/app-production-debug.apk
   adb shell pm grant at.exponential android.permission.POST_NOTIFICATIONS
   ```
   (Installing over a store build of Exponential fails on the signature:
   uninstall it first, `adb uninstall at.exponential`.)

## Timing

3. In a second terminal: `adb logcat -c && adb logcat -s VappSpike`.
4. Open the kitchen sink:
   ```bash
   adb shell am start -n at.exponential/com.exponential.app.MainActivity --es exp.devScreen kitchen-sink
   ```
   Signed out, it opens over the sign-in screen; signed in, it is pushed onto
   the app. Back closes it.
5. The caption top-right reads `N nodes · calls · taffy µs · wall µs`. The
   FIRST line in logcat is the cold pass (JNA load, first text layout, JIT).
   Tap the caption 5 times, waiting a second between taps: every tap is a
   full re-measure pass (the surface width toggles by 0.01 dp so taffy's
   measure cache misses; without that a re-layout makes 0 measure calls).
   Write down all 5 readings plus the first cold line. The logcat line also
   carries `host_ns` (time inside the Kotlin measure callback) and
   `host_hits` (per-pass memo hits); please copy those too.
6. Force-stop (`adb shell am force-stop at.exponential`), then the bench:
   ```bash
   adb shell am start -n at.exponential/com.exponential.app.MainActivity --es exp.devScreen kitchen-sink-bench
   ```
   Same readings: the cold line + 5 caption taps.
7. Optional RTL look: `--es exp.devScreen kitchen-sink-rtl`, screenshot it
   (`adb exec-out screencap -p > rtl.png`).

Log line fields: `nodes calls taffy_ns (inside the core, incl. callbacks)
ffi_ns (around the FFI call) wall_ns (the whole Compose measure pass incl.
the final measure()s) build_ns (parse + tree build)`.

## TalkBack sweep

8. Settings → Accessibility → TalkBack → on (or
   `adb shell settings put secure enabled_accessibility_services com.google.android.marvin.talkback/com.google.android.marvin.talkback.TalkBackService`).
9. Open the kitchen sink (step 4). Touch the avatar at the top left once,
   then swipe RIGHT with one finger repeatedly to the bottom of the screen
   (TalkBack scrolls on its own). Write down every spoken item in order.
10. Expected (fixture pre-order): Alex Chen · Reddit radar · Kitchen sink · one
    taffy layout on every client · 3 · Scan now · Sources · r/selfhosted
    312 posts · r/opensource 88 posts · r/webdev 1.2k posts · Auto-scan
    (switch, on) · Drafts · Cover · LIVE · Looking for a Linear alternative …
    · Draft reply ready · 2 sources cited · Progress 62% · All · Drafts · Sent ·
    Archived · More · Draft reply · Title (echoes after 150 ms) · host: ·
    Body · r/selfhosted · Cancel · Send · basis 30% · grow 1 · 160 · shrink 0
    · grow 2 · max 50% · 25%. Mark every place the spoken order differs
    (TalkBack sorts by geometry, so the header row and the flex-demo row are
    the likely spots).
11. TalkBack off again (same setting, or `settings put secure enabled_accessibility_services ""`).

## Typing by hand

12. Tap the "Title (echoes after 150 ms)" field and type
    `abcdefghijklmnopqrstuvwxyz0123456789ABCD` as fast as you can on the
    on-screen keyboard. Wait a second. Pass = the field shows exactly the
    string and the caption under it reads `host: ` + the same string. Note any
    dropped, duplicated or reordered character, and whether the caret ever
    jumped.
13. Repeat with `adb shell input text abcdefghijklmnopqrstuvwxyz0123456789ABCD`
    while the field is focused.

Send back: device model + Android version, the cold + 5 readings for both
screens, the spoken order, the typing results.
