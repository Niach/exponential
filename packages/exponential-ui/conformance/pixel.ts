// The PERCEPTUAL step of the conformance harness (reported, never gating):
// per case, the web surface as Chromium paints it (the conformance page,
// Playwright element screenshot, deviceScaleFactor 1) next to the gpui
// painter in a REAL window under Xvfb (`conformance_dump --window`, wgpu on
// the machine's Vulkan — lavapipe works — and a FONTCONFIG_FILE that lists
// only the conformance fonts), compared over their common top-left area:
// the share of pixels pixelmatch calls different (threshold 0.1) and a mean
// SSIM over 8×8 luminance blocks. Files land in `<out>/pixels/`; nothing is
// written under shots/ (the committed store shots stay with
// packages/exponential-ui-react/scripts/shots.ts).

import { spawnSync } from "node:child_process"
import { existsSync, mkdirSync, readFileSync, symlinkSync, writeFileSync, rmSync } from "node:fs"
import { basename, join } from "node:path"
import pixelmatch from "pixelmatch"
import sharp from "sharp"
import { fontFiles, MANIFEST_PATH, type ConformanceManifest, type Dump, type FontManifest } from "./dump"

export interface PixelRow {
  key: string
  diffRatio?: number
  ssim?: number
  size?: string
  files?: string
  note?: string
}

export interface Raster {
  width: number
  height: number
  data: Uint8Array
}

async function load(path: string): Promise<Raster> {
  const { data, info } = await sharp(path).ensureAlpha().raw().toBuffer({ resolveWithObject: true })
  return { width: info.width, height: info.height, data: new Uint8Array(data.buffer, data.byteOffset, data.length) }
}

/** The top-left `w × h` of a raster. */
export function crop(r: Raster, w: number, h: number): Raster {
  const out = new Uint8Array(w * h * 4)
  for (let y = 0; y < h; y++) out.set(r.data.subarray(y * r.width * 4, y * r.width * 4 + w * 4), y * w * 4)
  return { width: w, height: h, data: out }
}

const luma = (d: Uint8Array, i: number) => 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2]

/** Mean SSIM over non-overlapping 8×8 luminance blocks (same size inputs). */
export function ssim(a: Raster, b: Raster, block = 8): number {
  const c1 = (0.01 * 255) ** 2
  const c2 = (0.03 * 255) ** 2
  let sum = 0
  let n = 0
  for (let by = 0; by + block <= a.height; by += block)
    for (let bx = 0; bx + block <= a.width; bx += block) {
      let ma = 0
      let mb = 0
      for (let y = by; y < by + block; y++)
        for (let x = bx; x < bx + block; x++) {
          const i = (y * a.width + x) * 4
          ma += luma(a.data, i)
          mb += luma(b.data, i)
        }
      const k = block * block
      ma /= k
      mb /= k
      let va = 0
      let vb = 0
      let cov = 0
      for (let y = by; y < by + block; y++)
        for (let x = bx; x < bx + block; x++) {
          const i = (y * a.width + x) * 4
          const da = luma(a.data, i) - ma
          const db = luma(b.data, i) - mb
          va += da * da
          vb += db * db
          cov += da * db
        }
      va /= k - 1
      vb /= k - 1
      cov /= k - 1
      sum += ((2 * ma * mb + c1) * (2 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2))
      n++
    }
  return n ? sum / n : 1
}

/** Compare two PNGs over their common area; writes the pixelmatch diff. */
export async function comparePngs(webPng: string, desktopPng: string, diffPng: string): Promise<{ diffRatio: number; ssim: number; size: string }> {
  const a = await load(webPng)
  const b = await load(desktopPng)
  const w = Math.min(a.width, b.width)
  const h = Math.min(a.height, b.height)
  const ca = crop(a, w, h)
  const cb = crop(b, w, h)
  const diff = new Uint8Array(w * h * 4)
  const changed = pixelmatch(ca.data, cb.data, diff, w, h, { threshold: 0.1 })
  await sharp(Buffer.from(diff), { raw: { width: w, height: h, channels: 4 } })
    .png()
    .toFile(diffPng)
  return { diffRatio: changed / (w * h), ssim: ssim(ca, cb), size: `web ${a.width}×${a.height} · gpui ${b.width}×${b.height} · compared ${w}×${h}` }
}

const has = (cmd: string) => spawnSync(`sh`, [`-c`, `command -v ${cmd}`]).status === 0

/** A fontconfig root holding ONLY the conformance font files (+ the last resort). */
function fontconfigDir(repoRoot: string, dir: string): string {
  const manifest = JSON.parse(readFileSync(join(repoRoot, MANIFEST_PATH), `utf8`)) as ConformanceManifest
  const fonts = JSON.parse(readFileSync(join(repoRoot, manifest.fonts), `utf8`)) as FontManifest
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(join(dir, `fonts`), { recursive: true })
  const files = fontFiles(fonts).map((f) => join(repoRoot, f))
  for (const family of fonts.lastResort?.families ?? []) {
    const r = spawnSync(`fc-match`, [`-f`, `%{family}\n%{file}`, family], { encoding: `utf8` })
    const [found, file] = (r.stdout ?? ``).split(`\n`)
    if (file && found?.split(`,`).some((f) => f.trim() === family)) {
      files.push(file.trim())
      break
    }
  }
  for (const f of files) symlinkSync(f, join(dir, `fonts`, basename(f)))
  const conf = join(dir, `fonts.conf`)
  writeFileSync(conf, `<?xml version="1.0"?>\n<!DOCTYPE fontconfig SYSTEM "fonts.dtd">\n<fontconfig><dir>${join(dir, `fonts`)}</dir><cachedir>${join(dir, `cache`)}</cachedir></fontconfig>\n`)
  return conf
}

export async function pixelStep(repoRoot: string, outDir: string, keys: string[]): Promise<PixelRow[]> {
  const dir = join(outDir, `pixels`)
  mkdirSync(dir, { recursive: true })
  if (!keys.length) return []
  // Web: the conformance page, one element screenshot per case.
  const web = spawnSync(`bun`, [`scripts/conformance-web.ts`, `--keys`, keys.join(`,`), `--shots`, dir, `--out`, join(dir, `web-shots.json`)], { cwd: join(repoRoot, `packages/exponential-ui-react`), stdio: [`ignore`, `inherit`, `inherit`] })
  const desktopDump = join(outDir, `desktop.json`)
  const heights = existsSync(desktopDump) ? (JSON.parse(readFileSync(desktopDump, `utf8`)) as Dump).cases : {}
  const missing = [`xvfb-run`, `xdotool`, `import`].filter((c) => !has(c))
  const canDesktop = (process.platform === `linux` || process.platform === `freebsd`) && missing.length === 0
  let built = false
  if (canDesktop) built = spawnSync(`cargo`, [`build`, `-q`, `-p`, `exponential-ui-gpui`, `--example`, `conformance_dump`], { cwd: join(repoRoot, `apps/desktop`), stdio: [`ignore`, `inherit`, `inherit`] }).status === 0
  const conf = canDesktop ? fontconfigDir(repoRoot, join(dir, `fontconfig`)) : ``
  const rows: PixelRow[] = []
  for (const key of keys) {
    const slug = key.replaceAll(`/`, `_`)
    const webPng = join(dir, `${slug}.web.png`)
    if (web.status !== 0 || !existsSync(webPng)) {
      rows.push({ key, note: `web screenshot failed` })
      continue
    }
    if (!canDesktop) {
      rows.push({ key, note: `desktop capture unavailable here (${missing.length ? `missing ${missing.join(`, `)}` : process.platform}); web shot ${webPng}` })
      continue
    }
    if (!built) {
      rows.push({ key, note: `the conformance_dump example did not build` })
      continue
    }
    const width = Number(key.split(`/`)[3])
    const height = Math.min(8000, Math.ceil(heights[key]?.height ?? 2000))
    const deskPng = join(dir, `${slug}.desktop.png`)
    rmSync(deskPng, { force: true })
    // The window presents its first frame on input: move the pointer once.
    const script = [
      `cargo run -q -p exponential-ui-gpui --example conformance_dump -- --window '${key}' --height ${height} & P=$!`,
      `for i in $(seq 1 240); do W=$(xdotool search --name '^xui-conformance$' 2>/dev/null | head -1); [ -n "$W" ] && break; sleep 0.5; done`,
      `sleep 3; xdotool mousemove ${width + 5} 5; sleep 1; xdotool mousemove ${width + 6} 6; sleep 2`,
      `[ -n "$W" ] && import -window "$W" '${deskPng}'`,
      `kill $P`,
    ].join(`\n`)
    const r = spawnSync(`xvfb-run`, [`-a`, `-s`, `-screen 0 ${width + 16}x${height + 16}x24 +extension RENDER +extension Composite`, `sh`, `-c`, script], { cwd: join(repoRoot, `apps/desktop`), env: { ...process.env, FONTCONFIG_FILE: conf }, encoding: `utf8`, timeout: 300_000 })
    if (!existsSync(deskPng)) {
      rows.push({ key, note: `desktop capture failed (xvfb-run exit ${r.status}): ${(r.stderr ?? ``).split(`\n`).filter((l) => l && !l.includes(`libEGL`)).slice(-3).join(` | `)}` })
      continue
    }
    const m = await comparePngs(webPng, deskPng, join(dir, `${slug}.diff.png`))
    rows.push({ key, ...m, files: `${slug}.{web,desktop,diff}.png` })
  }
  return rows
}
