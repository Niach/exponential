# Exponential UI samples (VAPP-91)

Six hosts OUTSIDE the monorepo's build graph render the same surface,
streamed from a local A2UI server, in a third-party theme, with one custom
extension component. No Exponential account, no backend of ours: the SDK's
public API only.

| path | host | consumes the SDK as |
|---|---|---|
| `server/server.ts` | the A2UI server: `/a2ui.jsonl` (`?once=1`), `/a2ui.sse`, `/ws`, `POST /action`, `/theme.json`, `/extension.json` | Bun, no dependencies |
| `shared/` | `surface.jsonl` (the `greenhouse` surface), `sample.theme.json` (extends `neutral`: teal primary, pill buttons), `sample.extension.json` (`TrendLine`) | data |
| `web/` | a plain Vite + React app (`ExponentialHost` + `JsonlStreamTransport` + `<HostSurface>`) | npm tarballs (`pack-local.ts`; `^0.1.0` from npm once published) |
| `web-solid/` | the same app on Vite + Solid (`@exponential_app/eui-solid`'s `<HostSurface>`, port 4192) | the same tarballs |
| `web-element/` | the same page with no framework: `<exponential-surface>` with `src` + `post` (`@exponential_app/eui-element`, port 4193) | the same tarballs |
| `ios/` | a blank Xcode app | the Swift package by path (the SwiftPM URL once published) |
| `android/` | a blank Android Studio project | `at.exponential:eui-compose:0.2.0` from `mavenLocal()` (Maven Central once published) |
| `gpui/` | a plain gpui window, its own Cargo workspace | path dependencies standing in for crates.io |

```bash
bun samples/exponential-ui/server/server.ts          # http://localhost:4190
bun samples/exponential-ui/pack-local.ts             # web: stage + pack the npm packages into .packs/
cd samples/exponential-ui/web && npm install && npx vite   # http://localhost:4191 (?mode=dark, ?once=1)
cd samples/exponential-ui/web-solid && npm install && npx vite   # http://localhost:4192
cd samples/exponential-ui/web-element && npm install && npx vite # http://localhost:4193
```

Each native sample's README says how to build and run it. Every host
fetches the theme and the extension catalog at runtime, registers a native
`TrendLine` painter, connects the JSONL transport, sends actions back
(`Refresh` → the server pushes new readings) and routes `Docs`
(`functionCall openUrl`) through the URL policy.
