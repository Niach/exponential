# @exp/licenses

Third-party notice generation (EXP-375). Produces one committed `NOTICES.txt`
per distributed client, generated from the real dependency graphs and gated so
it cannot go stale.

| Client    | Output                                        | Ships via                         |
| --------- | --------------------------------------------- | --------------------------------- |
| Desktop   | `apps/desktop/assets/licenses/NOTICES.txt`    | embedded in the binary            |
| Web       | `apps/web/public/NOTICES.txt`                 | Nitro static, `GET /NOTICES.txt`  |
| Marketing | `apps/marketing/public/NOTICES.txt`           | vite copies `public/`             |
| iOS       | `apps/ios/Exponential/Resources/NOTICES.txt`  | the `Resources/**` Tuist glob     |
| Android   | `apps/android/app/src/main/assets/NOTICES.txt`| AGP packages `src/main/assets/`   |

Plain `.txt`, not markdown: licence bodies are full of `*`, `#`, `---` and
indented blocks that a renderer mangles, and altering reproduced text is exactly
what these licences forbid.

## The two halves

**Collect** needs toolchains. **Merge** must not.

```
collect:rust     cargo metadata × 3 target triples   -> inventory/rust.json
                                                        inventory/rust-excluded.json
collect:npm      node_modules production closure     -> inventory/npm-web.json
                                                        inventory/npm-marketing.json
collect:ios      GitHub raw @ the pinned revisions   -> inventory/ios.json
collect:android  gradle licenseProductionReleaseReport -> inventory/android.json

generate         inventories + curated/  ->  the five NOTICES.txt
```

`scripts/generate.ts` is pure and offline: it reads only committed files. That
is what lets `apps/web/src/lib/licenses.test.ts` re-run it in the pure-bun `web`
CI job and byte-compare the outputs on every PR.

The inventories cannot be regenerated there, so the same test proves them
against the lockfiles they came from instead — in both directions, so a removed
dependency cannot leave a stale notice behind.

## Commands

```bash
bun run --filter @exp/licenses generate          # the merge — no toolchain needed
bun run --filter @exp/licenses collect:rust      # needs cargo
bun run --filter @exp/licenses collect:npm       # needs node_modules
bun run --filter @exp/licenses collect:ios       # needs network
bun run --filter @exp/licenses collect:android   # needs a JDK + gradle
bun run --filter @exp/licenses fetch:texts       # refresh texts/ (needs network)
```

After changing a dependency in any client, re-run that client's collector and
`generate`, then commit both the inventory and the notice.

## Layout

- `inventory/` — **generated**. One file per ecosystem, in the `src/schema.ts`
  format. Never hand-edit; `.prettierignore` keeps prettier off them.
- `curated/supplement.ts` — the half no dependency graph knows about: bundled
  fonts, Lucide geometry copied by `packages/icons`, trademarks, vendored
  source, MPL-2.0 source URLs, and what was deliberately left out.
- `curated/overrides.ts` — dated determinations for packages whose metadata
  cannot be resolved mechanically. An override that matches nothing is a hard
  error, so these cannot go stale.
- `texts/spdx/` — canonical SPDX licence texts, fetched verbatim from
  `spdx/license-list-data` at a pinned tag. Used only as the body for a package
  that declares an id but ships no licence file.
- `texts/fonts/` — upstream font licences not already in the repo.
- `src/` — the shared engine: schema, SPDX expression parsing and the election
  policy, text normalisation, the renderer.

## Things that will bite you

**`Cargo.lock` is not the shipped graph.** It has ~90 more packages than we
ship. `libfuzzer-sys` is `(MIT OR Apache-2.0) AND NCSA` — NCSA is not on
`deny.toml`'s allow-list — and sits in the lock only as `rav1e`'s optional
fuzzing dependency. `collect:rust` walks the resolved, `--filter-platform`ed,
non-dev graph and writes the reason for every dropped package to
`inventory/rust-excluded.json` so the coverage gate stays mechanical.

**`OR` is a choice, not an obligation.** `self_cell` is
`Apache-2.0 OR GPL-2.0-only`. A generator that concatenated both branches would
assert GPL terms over our own binary. `src/spdx.ts` elects exactly one and
records it; `AND` keeps every conjunct, which is why `unicode-ident`
(`(MIT OR Apache-2.0) AND Unicode-3.0`) prints two.

**Determinism.** Codepoint sort only — never `localeCompare`, whose result
depends on the host's ICU build. No timestamps, hostnames or absolute paths.
About 55 npm packages are `os`/`cpu`-gated (`@esbuild/*`, `@rollup/rollup-*`,
`lightningcss-*`, `@remotion/compositor-*`) and only the host's own copy is ever
installed, so they are never read from disk — otherwise a macOS run and an
ubuntu run would disagree and the drift gate would fail forever.

**Dedupe by exact licence body.** Without it the desktop notice is several
megabytes of near-identical MIT text. Each component still prints its own
copyright line: that, not the permission text, is what MIT/BSD/ISC require.

**Never write licence text by hand.** Every body in a notice is reproduced from
a file — the package's own, one in this repository, or a canonical SPDX text
fetched at a pinned tag.
