# Android Macrobenchmarks

Frame timing for the journeys behind #EXP-1520..1522 ("omega lag"), measured against the app's `benchmark` build type: an R8 release build, profileable, signed with the debug key.

| Test | Journey | Watch |
|---|---|---|
| `transcriptScroll` | fast swipes up and down a long live transcript | `frameDurationCpuMs`, `frameOverrunMs` |
| `openSession` | cold open of the thread, then the session: the full join replay | `sessionOpenFrameMaxMs`, `sessionOpenMainThreadMaxMs` (the frames after the card tap), `SteerConnection.commitStagingSumMs` |
| `sendThenBack` | type, send, press back at once | `Choreographer#doFrame%MaxMs` (the old 2–3 s freeze), `frameDurationCpuMs` |
| `typeWhileLive` | type 60 characters into a live session's composer while the agent's `turn` frames tick | fails if a character is lost or reordered; `frameDurationCpuMs` |
| `reopenSessionDock` | back to the Conversation tab, then reopen the session's card (#EXP-1538) | `Dock.openToVisibleMaxMs`: open → the first Dock card |
| `coldOpenSessionDock` | the same on a cold process | `Dock.openToVisibleMaxMs` |

## Setup (local stack + emulator)

1. Backend: `bun run backend`, then the screenshot seed and a long feed. From `apps/web`:

   ```bash
   bun run seed:screenshots --shared-dev-db
   TMPDIR=/tmp/<you>-tmp/ SCREENSHOT_FEED_QUEUED=1 SCREENSHOT_FEED_REPEAT=40 SCREENSHOT_FEED_NO_QUESTION=1 SCREENSHOT_FEED_TURN_TICKS=3600 bun run screenshots:desktop
   ```

   Leave the stub running: its eNet node serves the 40× scripted feed for the demo session, then one live `turn` tick per second (a working agent's clock, tokens and "Writing…"). `SCREENSHOT_FEED_QUEUED=1` keeps one queued message on the session: since S-3 a waiting plan or question is not a Dock card, so the Dock journeys need this synced card to measure. A stub started without the flag removes it. Give the stub its own `TMPDIR`: every stub otherwise shares one eNet identity, and the app may dial another worktree's stub.
2. Web server: the emulator must be a trusted origin. Run vite with `BETTER_AUTH_TRUSTED_ORIGINS` including `http://10.0.2.2:<port>`.
3. Build and install the app:

   ```bash
   cd apps/android
   ./gradlew :app:assembleProductionBenchmark :macrobenchmark:assembleProductionBenchmark
   adb install -r app/build/outputs/apk/production/benchmark/app-production-benchmark.apk
   adb install -r -t macrobenchmark/build/outputs/apk/production/benchmark/macrobenchmark-production-benchmark.apk
   ```

4. Sign the benchmark app in once, by hand: self-hosted instance `http://10.0.2.2:<port>`, `demo@exponential.at` / `screenshots-demo`. Grant the notification permission (`adb shell pm grant at.exponential android.permission.POST_NOTIFICATIONS`), or its prompt covers the thread on the first cold start.
5. Run:

   ```bash
   adb shell am instrument -w \
     -e class com.exponential.macrobenchmark.SessionBenchmarks \
     -e androidx.benchmark.suppressErrors EMULATOR \
     com.exponential.macrobenchmark/androidx.test.runner.AndroidJUnitRunner
   ```

   `-e threadId <root id> -e sessionTitle <card title>` points the journeys at another thread. The traces land in `/storage/emulated/0/Android/media/com.exponential.macrobenchmark/`.

Compare numbers on the same device only. Emulator numbers are for before/after, not for absolute frame budgets.

### Reading `openSession`

`Choreographer#doFrame%MaxMs` there is the cold process's FIRST frame, not the session's: the window's EGL context and surface, plus every Skia shader compiled afresh (a cold start drops the shader cache). On an emulator without a host GPU (`hw.gpu.enabled=no`, the AVD default here) that frame is ~0.5 s of RenderThread `shader_compile` with the main thread asleep in `postAndWait`; with `-gpu host` it is ~0.2 s. No session code runs in it. `SessionOpenFrameMetric` reads only the frames after the card tap:

- `sessionOpenFrameMaxMs`: the longest of them (wall; includes waiting on the RenderThread, i.e. the emulator's GL).
- `sessionOpenMainThreadMaxMs`: the most main-thread CPU any one of them took: composition, layout, the feed fold.
- `coldFirstFrameMs`: the first frame, for comparison.

Run the emulator with a host GPU (`emulator -avd <avd> -gpu host`) for frame numbers; software GL turns every first-seen draw op into a 30–100 ms shader compile. A journey that fails with `agent-feed not found` sat on "No live transcript": the app's eNet node had not come up (logcat `EnetNodes`), so the session never joined the stub (#EXP-1590: after a cold restart the node can stay down ~80 s).
