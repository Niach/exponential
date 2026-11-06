# The web runner (E6d, #VAPP-269)

The static PWA an exported app runs in: the Engine's TS twin (`@exp/eengine`,
one `AppInstance`), the React genUI renderer, SQLite-WASM on OPFS (IndexedDB
when OPFS's sync access handles are missing), files in OPFS, the Vault in
IndexedDB, the web capabilities (`@exp/eengine/web`). Its contract is
`packages/eengine/fixtures/runner.json`; it has no chrome of its own (every
pixel is a genUI surface), no server, no OTA (a new version is a new export),
no telemetry and no account.

```
bun run build      # dist/ = the whole export: the shell, app/, manifest + icons, sw.js
bun run preview    # serve dist/
bun run test       # the controller with fakes
bun run capture -- --package samples/album.app.json --name album --permissions --out /tmp/shots
```

Icons: the JS bundles no icon set. `vite build` writes `glyphs.json` (every
Lucide glyph by name + Lucide's aliases + icons.json `semantic`,
`scripts/glyphs.ts`); the page loads it before the first paint (`src/glyphs.ts`)
and `exponential bundle` prunes it to the names the package and the shell use.
The build also drops sqlite-wasm's Worker1 script (the records open in
`src/sqlite-worker.ts`).

`dist/app/` holds `runner.json` and `package.app.json` (here: `samples/album.app.json`);
E7's `exponential bundle --target web` replaces them, the web manifest, the icons and the favicon, prunes `glyphs.json` and rewrites `sw.js`. The
service worker precaches every file of the export, so the app works offline;
its cache name hashes them all, so a new export replaces the old cache.
`src/page.ts` drives lifecycle.json from the page (launch, `visibilitychange`,
`pagehide`/`pageshow`, `online`, `?screen=…` / `#screen?…` deep links,
notification taps) and re-installs the shell when the window crosses genUI's
`md` breakpoint.
