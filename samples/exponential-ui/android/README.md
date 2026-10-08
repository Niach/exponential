# Greenhouse: the Android sample host

A standalone Android Studio project (AGP 8.10, Kotlin 2.4, minSdk 26) that hosts an Exponential UI surface the way any third-party app would:
- `at.exponential:ui-compose:0.1.0` from a Maven repository;
- a surface streamed from the local A2UI JSONL server;
- the server's custom theme (teal primary, pill buttons);
- ONE custom extension component, `TrendLine`, painted natively with a Compose `Canvas`.

No Exponential account, no backend of ours. It is the Compose twin of `../web`.

```bash
bun samples/exponential-ui/server/server.ts &                       # http://localhost:4190
cd packages/exponential-ui-compose && ./gradlew :ui-compose:publishReleasePublicationToMavenLocal :ui-compose-primitives:publishReleasePublicationToMavenLocal
cd samples/exponential-ui/android && ./gradlew :app:installDebug      # needs local.properties (sdk.dir)
adb shell am start -n at.exponential.samples.greenhouse/.MainActivity
```

- **Before the Maven Central release**, the SDK resolves from `mavenLocal()`.
- **What `MainActivity.kt` does:**
  - fetches `/theme.json` and `/extension.json`;
  - builds an `ExponentialHost` with a `JsonlStreamTransport` (`postUrl` = `/action`) and the extension plus its `TrendLinePainter`;
  - paints `HostSurface(host, "greenhouse")`.
- **Network.** The emulator reaches the host machine at `10.0.2.2`. Cleartext is allowed for `10.0.2.2` and `localhost` only (`res/xml/network_security_config.xml`). Use `adb reverse tcp:4190 tcp:4190` with `--es server http://localhost:4190` on a device.

| launch extra | effect |
|---|---|
| `--es server <url>` | the sample server (default `http://10.0.2.2:4190`) |
| `--ez live true` | keep the stream open for the live readings (default: `?once=1`, the surface only, then `transport: closed`) |
| `--es mode dark` | dark mode (default light) |
