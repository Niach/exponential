#!/usr/bin/env bun
// `bun run --filter @exponential-at/ui conformance` — the renderer
// conformance harness, everything this machine can run:
//   1. the WEB dump: the React renderer in headless Chromium with the
//      conformance fonts (packages/exponential-ui-react/scripts/conformance-web.ts),
//      compared with the committed baseline (fixtures/conformance-baseline.json)
//   2. the DESKTOP dump: the gpui painter headless with gpui's own Linux text
//      system and the same font files (cargo example `conformance_dump` of
//      apps/desktop/crates/exponential-ui-gpui), compared with the baseline
//   3. (`--pixels`) the perceptual step: the web surface PNG next to the
//      desktop window's capture (Xvfb), a pixel-difference ratio + a global
//      SSIM per case — REPORTED, never gating
// and prints the tables, the divergence origins grouped across cases and
// writes the dumps + a JSON report to `--out` (default $TMPDIR/xui-conformance).
//
//   --only <substring>   cases whose key contains it
//   --web <dump.json>    reuse a web dump      --skip-web
//   --desktop <dump>     reuse a desktop dump  --skip-desktop
//   --write-baseline     rewrite the baseline from the web dump (whole matrix)
//   --details            every case's divergence list (default: grouped)
//   --pixels [substring] the perceptual step (default: kitchen-sink/*/dark/900/ltr
//                        and responsive/*/dark/390/rtl)

import { spawnSync } from "node:child_process"
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { decodeBaseline, type Baseline } from "./baseline"
import { compareDumps, formatCase, formatTable, groupFindings, type Report } from "./compare"
import { allCases, BASELINE_PATH, MANIFEST_PATH, type ConformanceManifest, type Dump } from "./dump"

const repoRoot = join(import.meta.dir, `..`, `..`, `..`)
const argv = process.argv.slice(2)
const flag = (name: string) => argv.includes(`--${name}`)
const arg = (name: string) => {
  const i = argv.indexOf(`--${name}`)
  return i >= 0 && argv[i + 1] && !argv[i + 1].startsWith(`--`) ? argv[i + 1] : undefined
}
const outDir = arg(`out`) ?? join(tmpdir(), `xui-conformance`)
mkdirSync(outDir, { recursive: true })
const manifest = JSON.parse(readFileSync(join(repoRoot, MANIFEST_PATH), `utf8`)) as ConformanceManifest
const only = arg(`only`)
const keys = allCases(manifest)
  .map((c) => c.key)
  .filter((k) => !only || k.includes(only))
const tol = manifest.tolerance

function run(cmd: string, args: string[], cwd: string): boolean {
  console.error(`$ ${cmd} ${args.join(` `)}`)
  const r = spawnSync(cmd, args, { cwd, stdio: [`ignore`, `inherit`, `inherit`], env: process.env })
  if (r.error) console.error(String(r.error))
  return r.status === 0
}

const readDump = (path: string) => JSON.parse(readFileSync(path, `utf8`)) as Dump
const baselinePath = join(repoRoot, BASELINE_PATH)
const loadBaseline = () => (existsSync(baselinePath) ? decodeBaseline(JSON.parse(readFileSync(baselinePath, `utf8`)) as Baseline) : null)

// 1. web
let web: Dump | null = null
const webPath = arg(`web`) ?? join(outDir, `web.json`)
if (!flag(`skip-web`)) {
  if (!arg(`web`)) {
    const args = [`scripts/conformance-web.ts`, `--out`, webPath]
    if (only) args.push(`--only`, only)
    if (flag(`write-baseline`)) args.push(`--write-baseline`)
    if (!run(`bun`, args, join(repoRoot, `packages/exponential-ui-react`))) console.error(`web dump FAILED (is Chromium installed? bunx playwright install chromium)`)
  }
  if (existsSync(webPath)) web = readDump(webPath)
}
const baseline = loadBaseline()
const reports: Record<string, Report> = {}
if (!baseline) console.error(`no baseline at ${BASELINE_PATH}: run with --write-baseline`)
if (web && baseline) {
  reports.web = compareDumps(baseline, web, tol, keys)
  console.log(`\n# web vs the committed baseline\n`)
  console.log(formatTable(reports.web))
}

// 2. desktop
const desktopPath = arg(`desktop`) ?? join(outDir, `desktop.json`)
if (!flag(`skip-desktop`)) {
  if (!arg(`desktop`)) {
    const args = [`run`, `-q`, `-p`, `exponential-ui-gpui`, `--example`, `conformance_dump`, `--`, `--out`, desktopPath]
    if (only) args.push(`--only`, only)
    if (process.platform !== `linux` && process.platform !== `freebsd`) console.error(`desktop dump: Linux/FreeBSD only for now — skipped`)
    else if (!run(`cargo`, args, join(repoRoot, `apps/desktop`))) console.error(`desktop dump FAILED`)
  }
  if (existsSync(desktopPath) && baseline) {
    reports.desktop = compareDumps(baseline, readDump(desktopPath), tol, keys)
    console.log(`\n# desktop (gpui, headless) vs the web baseline\n`)
    console.log(formatTable(reports.desktop))
    const findings = groupFindings(reports.desktop)
    console.log(`\n## divergence origins across cases (${findings.length}; Δ = desktop − web)\n`)
    for (const f of findings.slice(0, flag(`details`) ? findings.length : 80))
      console.log(`  ${String(f.cases).padStart(3)}× ${f.kinds.padEnd(13)} ${f.id} (${f.component})  Δw ${f.dw[0]}…${f.dw[1]}  Δh ${f.dh[0]}…${f.dh[1]}${f.text ? `  "${f.text.slice(0, 32)}"` : ``}\n        e.g. ${f.example}`)
    if (!flag(`details`) && findings.length > 80) console.log(`  … ${findings.length - 80} more (--details)`)
    if (flag(`details`)) for (const c of reports.desktop.cases) console.log(`\n${formatCase(c)}`)
  }
}

// 3. pixels
if (flag(`pixels`)) {
  const sel = arg(`pixels`)
  const pixelKeys = keys.filter((k) => (sel ? k.includes(sel) : /^(kitchen-sink\/[^/]+\/dark\/900\/ltr|responsive\/[^/]+\/dark\/390\/rtl)$/.test(k)))
  const { pixelStep } = await import(`./pixel`)
  const rows = await pixelStep(repoRoot, outDir, pixelKeys)
  console.log(`\n# perceptual (reported, never gating)\n`)
  for (const r of rows) console.log(`  ${r.key.padEnd(44)} ${r.note ?? `diff ${(r.diffRatio! * 100).toFixed(2)}%  SSIM ${r.ssim!.toFixed(4)}  (${r.size})  ${r.files}`}`)
}

writeFileSync(join(outDir, `report.json`), JSON.stringify(reports, null, 1))
console.log(`\ndumps + report.json in ${outDir}`)
const webBad = reports.web ? reports.web.cases.some((c) => c.diffs.length || c.onlyRef.length || c.onlyCand.length) || reports.web.missingCases.length > 0 : false
if (webBad) {
  console.log(`\nthe WEB dump drifted from the baseline (see above): fix the renderer, or rewrite the baseline with --write-baseline when the change is intended`)
  process.exitCode = 1
}
