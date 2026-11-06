#!/usr/bin/env bun
// #EDE-45: the parity contact sheet. Every region of the IDE's shell next to
// Exponential Desktop's, cropped from the committed shots (the IDE's
// `desktop.webp` at its 1.25× store scale, eDE's `ede.webp` at 1×), scaled to
// one height, one row per region with its name and the status
// `packages/uicore/parity.md` gives it. Danny signs the sheet off by eye; the
// gate rule (≥ 90 % of the inventory's rows `same` or `close`) is checked
// here too and printed.
//
//   bun apps/ede/scripts/parity.ts [--out /tmp/parity.png]
import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"
import sharp from "sharp"

const root = join(import.meta.dir, `../../..`)
const out = process.argv.includes(`--out`) ? process.argv[process.argv.indexOf(`--out`) + 1] : `/tmp/ede-parity.png`

/** A crop in the shot's own pixels: `left, top, width, height`. */
type Crop = [number, number, number, number]
interface Region {
  name: string
  /** The IDE's store shot (shots/<view>/desktop.webp) and the crop in it. */
  ide: { view: string; crop: Crop }
  /** eDE's store shot (shots/<view>/ede.webp) and the crop in it. */
  ede: { view: string; crop: Crop }
}

// The IDE's desktop shots are 1800 × 1126 for a 1440 × 900 window (1.25×);
// eDE's goldens are 1280 × 720 at 1×.
const IDE = 1.25
const REGIONS: Region[] = [
  { name: `The frame: black ground, the rounded content panel`, ide: { view: `agent-home`, crop: [0, 0, 1800, 1126] }, ede: { view: `ede-panel`, crop: [0, 0, 1280, 720] } },
  { name: `The sidebar column (272 px)`, ide: { view: `agent-home`, crop: [0, 0, 272 * IDE, 1126] }, ede: { view: `ede-panel`, crop: [0, 0, 272, 720] } },
  { name: `The sidebar header: switcher, inbox, search, new`, ide: { view: `agent-home`, crop: [0, 50, 272 * IDE, 60] }, ede: { view: `ede-panel`, crop: [0, 4, 272, 48] } },
  { name: `Nav rows and sections`, ide: { view: `agent-home`, crop: [0, 125, 272 * IDE, 420] }, ede: { view: `ede-panel`, crop: [0, 56, 272, 270] } },
  { name: `The bottom-left corner`, ide: { view: `agent-home`, crop: [0, 1040, 272 * IDE, 86] }, ede: { view: `ede-panel`, crop: [0, 664, 272, 56] } },
  { name: `The icon rail (eDE only; the IDE's sidebar never folds)`, ide: { view: `agent-home`, crop: [0, 0, 56 * IDE, 1126] }, ede: { view: `ede-rail`, crop: [0, 0, 56, 720] } },
  { name: `The work tabs band (IDE) vs Mint's taskbar (eDE, by decision)`, ide: { view: `agent-home`, crop: [340, 0, 1460, 55] }, ede: { view: `ede-panel`, crop: [272, 672, 1008, 48] } },
  { name: `Alt-Tab: the IDE has none; eDE's switcher`, ide: { view: `agent-home`, crop: [340, 0, 1460, 1126] }, ede: { view: `ede-switcher`, crop: [255, 278, 770, 206] } },
  { name: `Settings navigation`, ide: { view: `settings-general`, crop: [0, 0, 400, 1126] }, ede: { view: `ede-settings`, crop: [160, 60, 960, 640] } },
]

const ROW_H = 220
const GAP = 16
const LABEL_H = 28
const SHEET_W = 1800

function statusOf(name: string, parity: string): string {
  const want = name.toLowerCase()
  const rows = parity.split(`\n`).filter((l) => l.startsWith(`| `) && /\| (same|close|gap) \|/.test(l))
  const hit = rows.find((l) => want.split(` `).slice(0, 2).every((w) => l.toLowerCase().includes(w)))
  return hit?.match(/\| (same|close|gap) \|/)?.[1] ?? ``
}

async function tile(file: string, crop: Crop): Promise<Buffer> {
  if (!existsSync(file)) {
    return sharp({ create: { width: 320, height: ROW_H, channels: 4, background: `#1a1a1a` } }).png().toBuffer()
  }
  const [left, top, width, height] = crop.map(Math.round) as Crop
  const meta = await sharp(file).metadata()
  const w = Math.min(width, (meta.width ?? width) - left)
  const h = Math.min(height, (meta.height ?? height) - top)
  return sharp(file)
    .extract({ left, top, width: Math.max(1, w), height: Math.max(1, h) })
    .resize({ height: ROW_H, width: Math.floor(SHEET_W / 2) - GAP, fit: `inside`, withoutEnlargement: false })
    .png()
    .toBuffer()
}

function label(text: string, width: number): Buffer {
  const svg = `<svg width="${width}" height="${LABEL_H}" xmlns="http://www.w3.org/2000/svg"><rect width="100%" height="100%" fill="#0a0a0a"/><text x="8" y="19" font-family="Inter, sans-serif" font-size="14" fill="#fafafa">${text.replace(/&/g, `&amp;`).replace(/</g, `&lt;`)}</text></svg>`
  return Buffer.from(svg)
}

async function main() {
  const parity = readFileSync(join(root, `packages/uicore/parity.md`), `utf8`)
  const rows = parity.split(`\n`).filter((l) => /\| (same|close|gap) \|/.test(l))
  const good = rows.filter((l) => /\| (same|close) \|/.test(l)).length
  const share = rows.length ? good / rows.length : 0
  const composites: sharp.OverlayOptions[] = []
  let y = 0
  for (const region of REGIONS) {
    const ide = await tile(join(root, `shots`, region.ide.view, `desktop.webp`), region.ide.crop)
    const ede = await tile(join(root, `shots`, region.ede.view, `ede.webp`), region.ede.crop)
    const status = statusOf(region.name, parity)
    composites.push({ input: label(`${region.name}${status ? `  ·  ${status}` : ``}   (left: the IDE, right: Exponential Desktop)`, SHEET_W), left: 0, top: y })
    y += LABEL_H
    const ideMeta = await sharp(ide).metadata()
    composites.push({ input: ide, left: 0, top: y })
    composites.push({ input: ede, left: Math.min(SHEET_W / 2, (ideMeta.width ?? 0) + GAP), top: y })
    y += ROW_H + GAP
  }
  const header = label(`eDE parity sheet (#EDE-45): ${good} of ${rows.length} inventory rows same or close (${Math.round(share * 100)} %, gate ≥ 90 %)`, SHEET_W)
  composites.unshift({ input: header, left: 0, top: 0 })
  for (const c of composites.slice(1)) c.top = (c.top as number) + LABEL_H
  y += LABEL_H
  await sharp({ create: { width: SHEET_W, height: y, channels: 4, background: `#0a0a0a` } }).composite(composites).png().toFile(out)
  console.log(`${out}: ${REGIONS.length} regions; inventory ${good}/${rows.length} (${Math.round(share * 100)} %) ${share >= 0.9 ? `passes` : `FAILS`} the gate`)
  if (share < 0.9) process.exit(1)
}

await main()
