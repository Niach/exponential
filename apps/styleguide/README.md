# @exp/styleguide

The gallery for the cross-platform screenshot store (EXP-566): every view in
`@exp/view-catalog`, with its web / web-mobile / desktop / iOS / Android shots
side by side, so a board on one platform can be compared with the same
board on the others.

Zero runtime dependencies. The output is ONE self-contained HTML file plus a
copy of `shots/` — `open dist/index.html` works with no server.

```bash
bun run dev:styleguide      # http://localhost:4173, re-renders from disk per request
bun run build:styleguide    # writes apps/styleguide/dist/
bun run shots:check         # build --check: fails on missing / undeclared shots
```

Per platform a view is either **captured**, **missing** or **n/a**:

- **missing** — the catalog declares a capture for that platform and nothing is
  in the store yet. Run the capture lane; `--check` exits 1 on these.
- **n/a** — the catalog declares no capture there. That is a deliberate gap and
  `views.json` `notes[platform]` says why; the placeholder shows that text.
- **undeclared** — a file in `shots/` that no view/platform pair claims. Either
  the catalog entry was renamed or the file is stale.

`SHOTS_DIR` points both commands at another store (scratch copies, tests);
by default it is the repo-root `shots/`.

Deploys as a Coolify STATIC app serving `apps/styleguide/dist` — no runtime, so
the build step is the whole deploy.

Keyboard: `j`/`k` or arrows move between views, `/` focuses the filter, clicking
a shot opens it 1:1, `Esc` closes.
