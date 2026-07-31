#!/usr/bin/env bun
// EXP-375 — materialises the canonical SPDX licence texts into `texts/spdx/`.
//
// Most packages ship their own LICENSE file and the collectors reproduce that
// verbatim. A meaningful minority ship none at all (56 of the 706 crates in the
// desktop graph, for instance) while still declaring an SPDX id. For those the
// notice has to reproduce the canonical text of the licence they named.
//
// The texts come from SPDX's own license-list-data repository at a PINNED tag,
// so they are authoritative and byte-stable. Nobody hand-writes licence text in
// this repo: altering reproduced terms is exactly what the licences forbid, and
// a plausible-looking paraphrase is worse than no notice at all.
//
// Run: `bun run --filter @exp/licenses fetch:texts` (needs network). The outputs
// are COMMITTED; this only needs re-running when a new id enters a graph.

import { mkdirSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const pkgRoot = join(dirname(fileURLToPath(import.meta.url)), "..")
const outDir = join(pkgRoot, "texts/spdx")

/** Pinned so a re-fetch two years from now produces the same bytes. */
const SPDX_TAG = `v3.28.0`

// Every id any collector can need a fallback body for. The union of
// apps/desktop/deny.toml's allow-list, the `OR` branches that appear in the
// four graphs (an unelected branch still has to be nameable), and the ids the
// npm/Android graphs contribute. Over-fetching is free; a missing template
// makes a collector fail loudly rather than invent text.
const IDS = [
  `0BSD`,
  `AFL-2.1`,
  `Apache-2.0`,
  `Artistic-2.0`,
  `BSD-2-Clause`,
  `BSD-3-Clause`,
  `BSL-1.0`,
  `BlueOak-1.0.0`,
  `CC-BY-3.0`,
  `CC-BY-4.0`,
  `CC0-1.0`,
  `CDDL-1.0`,
  `CDLA-Permissive-2.0`,
  `EPL-1.0`,
  `EPL-2.0`,
  `ISC`,
  `LLVM-exception`,
  `MIT`,
  `MIT-0`,
  `MPL-2.0`,
  `NCSA`,
  `OpenSSL`,
  `PSF-2.0`,
  `Python-2.0`,
  `Unicode-3.0`,
  `Unicode-DFS-2016`,
  `Unlicense`,
  `W3C`,
  `WTFPL`,
  `Zlib`,
  `bzip2-1.0.6`,
]

/** Same normalisation the collectors apply, so templates and shipped files
 *  dedupe against each other instead of differing by trailing whitespace. */
const normalise = (text: string): string =>
  text
    .replace(/^﻿/, ``)
    .replace(/\r\n?/g, `\n`)
    .split(`\n`)
    .map((line) => line.replace(/[ \t]+$/, ``))
    .join(`\n`)
    .replace(/\n+$/, ``) + `\n`

mkdirSync(outDir, { recursive: true })

let failed = 0
for (const id of IDS) {
  const url = `https://raw.githubusercontent.com/spdx/license-list-data/${SPDX_TAG}/text/${id}.txt`
  const res = await fetch(url)
  if (!res.ok) {
    console.error(`FAILED ${id}: ${res.status} ${url}`)
    failed++
    continue
  }
  writeFileSync(join(outDir, `${id}.txt`), normalise(await res.text()))
  console.log(`wrote ${id}.txt`)
}

writeFileSync(
  join(outDir, `SOURCE.txt`),
  `The .txt files in this directory are the canonical SPDX licence texts,\n` +
    `fetched verbatim from spdx/license-list-data at tag ${SPDX_TAG}:\n` +
    `\n` +
    `  https://github.com/spdx/license-list-data/tree/${SPDX_TAG}/text\n` +
    `\n` +
    `They are used only as the fallback body for a package that declares an\n` +
    `SPDX id but ships no licence file of its own. A package that DOES ship\n` +
    `one has that file reproduced instead. Regenerate with:\n` +
    `\n` +
    `  bun run --filter @exp/licenses fetch:texts\n` +
    `\n` +
    `Do not hand-edit these files.\n`
)

// ---------------------------------------------------------------------------
// Bundled fonts
// ---------------------------------------------------------------------------
//
// Inter and JetBrains Mono already have their upstream OFL reproduced in the
// repo (apps/desktop/assets/fonts/), and the curated supplement points at those
// files so there is exactly one copy of each. Geist is marketing-only and has
// no such file, so it is vendored here, pinned to an upstream commit.

const FONTS = [
  {
    family: `Geist`,
    file: `Geist-OFL.txt`,
    repo: `vercel/geist-font`,
    // Pinned: `main` moves, and a licence body must be reproducible forever.
    rev: `c40c1aec7e72b9ebdce65a4fccd03cb3950a359b`,
    path: `OFL.txt`,
  },
]

const fontsDir = join(pkgRoot, `texts/fonts`)
mkdirSync(fontsDir, { recursive: true })

for (const font of FONTS) {
  const url = `https://raw.githubusercontent.com/${font.repo}/${font.rev}/${font.path}`
  const res = await fetch(url)
  if (!res.ok) {
    console.error(`FAILED ${font.family}: ${res.status} ${url}`)
    failed++
    continue
  }
  writeFileSync(join(fontsDir, font.file), normalise(await res.text()))
  console.log(`wrote fonts/${font.file}`)
}

writeFileSync(
  join(fontsDir, `SOURCE.txt`),
  `Upstream font licences vendored verbatim, pinned to an upstream commit:\n` +
    `\n` +
    FONTS.map(
      (f) =>
        `  ${f.file}\n` +
        `    https://github.com/${f.repo}/blob/${f.rev}/${f.path}\n`
    ).join(``) +
    `\n` +
    `Inter and JetBrains Mono are NOT duplicated here — the curated supplement\n` +
    `reproduces apps/desktop/assets/fonts/LICENSE.txt and\n` +
    `apps/desktop/assets/fonts/JetBrainsMono-OFL.txt directly, so each font\n` +
    `licence has exactly one copy in the repository.\n` +
    `\n` +
    `Regenerate with: bun run --filter @exp/licenses fetch:texts\n` +
    `Do not hand-edit these files.\n`
)

if (failed > 0) {
  console.error(`\n${failed} text(s) failed to fetch`)
  process.exit(1)
}
console.log(
  `\n${IDS.length} SPDX templates and ${FONTS.length} font licence(s) written`
)
