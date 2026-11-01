# Android Macrobenchmarks

Frame timing for the journeys behind #EXP-1520..1522 ("omega lag"), measured against the app's `benchmark` build type: an R8 release build, profileable, signed with the debug key.

| Test | Journey | Watch |
|---|---|---|
| `transcriptScroll` | fast swipes up and down a long live transcript | `frameDurationCpuMs`, `frameOverrunMs` |
| `openSession` | cold open of the thread, then the session: the full join replay | `SteerConnection.commitStagingSumMs` (main thread), `Choreographer#doFrame%MaxMs` |
| `sendThenBack` | type, send, press back at once | `Choreographer#doFrame%MaxMs` (the old 2–3 s freeze), `frameDurationCpuMs` |
| `typeWhileLive` | type 60 characters into a live session's composer while the agent's `turn` frames tick | fails if a character is lost or reordered; `frameDurationCpuMs` |

## Setup (local stack + emulator)

1. Backend: `bun run backend`, then the screenshot seed and a long feed. From `apps/web`:

   ```bash
   bun run seed:screenshots --shared-dev-db
   SCREENSHOT_FEED_REPEAT=40 SCREENSHOT_FEED_NO_QUESTION=1 SCREENSHOT_FEED_TURN_TICKS=3600 bun run screenshots:desktop
   ```

   Leave the stub running: its eNet node serves the 40× scripted feed for the demo session, then one live `turn` tick per second (a working agent's clock, tokens and "Writing…").
2. Web server: the emulator must be a trusted origin. Run vite with `BETTER_AUTH_TRUSTED_ORIGINS` including `http://10.0.2.2:<port>`.
3. Build and install the app:

   ```bash
   cd apps/android
   ./gradlew :app:assembleProductionBenchmark :macrobenchmark:assembleProductionBenchmark
   adb install -r app/build/outputs/apk/production/benchmark/app-production-benchmark.apk
   adb install -r -t macrobenchmark/build/outputs/apk/production/benchmark/macrobenchmark-production-benchmark.apk
   ```

4. Sign the benchmark app in once, by hand: self-hosted instance `http://10.0.2.2:<port>`, `demo@exponential.at` / `screenshots-demo`.
5. Run:

   ```bash
   adb shell am instrument -w \
     -e class com.exponential.macrobenchmark.SessionBenchmarks \
     -e androidx.benchmark.suppressErrors EMULATOR \
     com.exponential.macrobenchmark/androidx.test.runner.AndroidJUnitRunner
   ```

   `-e threadId <root id> -e sessionTitle <card title>` points the journeys at another thread. The traces land in `/storage/emulated/0/Android/media/com.exponential.macrobenchmark/`.

Compare numbers on the same device only. Emulator numbers are for before/after, not for absolute frame budgets.
