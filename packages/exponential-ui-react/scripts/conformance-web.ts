#!/usr/bin/env bun
// The WEB half of the renderer conformance harness: every case of
// packages/exponential-ui/fixtures/conformance-cases.json rendered by the
// React renderer in headless Chromium with the conformance font set and
// dumped as frames.
//
//   bun scripts/conformance-web.ts [--out <dump.json>] [--only <substring>] [--keys <k1,k2>]
//       [--shots <dir>]          also a PNG of the surface per case
//       [--write-baseline]       (re)write fixtures/conformance-baseline.json
//
// `bun run --filter @exponential-at/ui conformance` calls this, then the
// desktop dump, then the comparator.

import { chromium } from "playwright"
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { allCases, MANIFEST_PATH, type ConformanceManifest } from "../../exponential-ui/conformance/dump"
import { encodeBaseline, BASELINE_FILE } from "../../exponential-ui/conformance/baseline"
import { CHROMIUM_ARGS, dumpCase, WEB_RENDERER } from "./conformance-drive"
import { repoRoot, startConformanceServer } from "./conformance-serve"

const argv = process.argv.slice(2)
const arg = (name: string) => {
  const i = argv.indexOf(`--${name}`)
  return i >= 0 ? argv[i + 1] : undefined
}
const manifest = JSON.parse(readFileSync(join(repoRoot, MANIFEST_PATH), `utf8`)) as ConformanceManifest
const only = arg(`only`)
const keys = arg(`keys`)?.split(`,`)
const cases = allCases(manifest).filter((c) => (!only || c.key.includes(only)) && (!keys || keys.includes(c.key)))
const shots = arg(`shots`)
if (shots) mkdirSync(shots, { recursive: true })

const server = startConformanceServer(4700 + Math.floor(Math.random() * 300))
const browser = await chromium.launch({ args: CHROMIUM_ARGS })
const producer = `Chromium ${browser.version()} headless (playwright), ${CHROMIUM_ARGS.join(` `)}, deviceScaleFactor 1`
const started = Date.now()
const dump = { format: `xui-frame-dump/1`, renderer: WEB_RENDERER, fonts: `conformance/fonts.json`, cases: {} as Record<string, import("../../exponential-ui/conformance/dump").CaseDump> }
try {
  for (const c of cases) {
    dump.cases[c.key] = await dumpCase(browser, server.url, c, { screenshot: shots ? join(shots, `${c.key.replaceAll(`/`, `_`)}.web.png`) : undefined })
    process.stderr.write(`.`)
  }
} finally {
  await browser.close()
  server.stop()
}
process.stderr.write(`\nconformance-web: ${cases.length} cases in ${((Date.now() - started) / 1000).toFixed(1)} s\n`)
const out = arg(`out`)
if (out) writeFileSync(out, JSON.stringify(dump))
if (argv.includes(`--write-baseline`)) {
  if (only || keys) throw new Error(`--write-baseline needs the whole matrix (no --only / --keys)`)
  writeFileSync(join(repoRoot, BASELINE_FILE), encodeBaseline(dump, producer))
  console.error(`wrote ${BASELINE_FILE}`)
}
if (!out && !argv.includes(`--write-baseline`)) console.log(JSON.stringify(dump))
