#!/usr/bin/env bun
/**
 * `bun run readme:hero` — compose `docs/images/hero.webp`, the README's one
 * showcase image, out of the COMMITTED shot store.
 *
 * It is deliberately a derivation, not a hand-made picture: the README showed
 * a retired sidebar (an "Agents" entry, a `todo` group) and a retired start
 * sheet for months because nothing regenerated it (EXP-770). Rebuild it after
 * a shots refresh and the README follows the product for free.
 *
 * The composition: the web board on the left as a rounded card with the app's
 * own hairline, the iOS Agent composer overlapping it on the right in a phone
 * bezel, both on the site's page gradient.
 */
import { mkdirSync } from "node:fs"
import { dirname, join } from "node:path"
import sharp from "sharp"

const ROOT = join(import.meta.dirname, `..`, `..`, `..`)
const OUT = join(ROOT, `docs`, `images`, `hero.webp`)

const W = 1920
const H = 1080
/* The site's page ground (styles.css body::before, the web/iOS stops). */
const TOP = `#050507`
const BOTTOM = `#111114`

/* Rounded-rect mask + hairline, drawn as SVG so one path does both. */
const rounded = (w: number, h: number, r: number): Buffer =>
  Buffer.from(
    `<svg width="${w}" height="${h}"><rect x="0" y="0" width="${w}" height="${h}" rx="${r}" ry="${r}" fill="#fff"/></svg>`
  )

const hairline = (w: number, h: number, r: number, alpha: number): Buffer =>
  Buffer.from(
    `<svg width="${w}" height="${h}"><rect x="0.5" y="0.5" width="${w - 1}" height="${h - 1}" rx="${r}" ry="${r}" fill="none" stroke="rgba(255,255,255,${alpha})" stroke-width="1"/></svg>`
  )

async function roundedShot(
  file: string,
  width: number,
  radius: number,
  stroke: number
): Promise<{ buffer: Buffer; width: number; height: number }> {
  const input = sharp(join(ROOT, file))
  const meta = await input.metadata()
  const height = Math.round((meta.height! / meta.width!) * width)
  const resized = await input.resize(width, height).png().toBuffer()
  const buffer = await sharp(resized)
    .composite([
      { input: rounded(width, height, radius), blend: `dest-in` },
      { input: hairline(width, height, radius, stroke), blend: `over` },
    ])
    .png()
    .toBuffer()
  return { buffer, width, height }
}

async function main(): Promise<void> {
  const background = Buffer.from(
    `<svg width="${W}" height="${H}">
       <defs>
         <linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
           <stop offset="0%" stop-color="${TOP}"/>
           <stop offset="100%" stop-color="${BOTTOM}"/>
         </linearGradient>
       </defs>
       <rect width="${W}" height="${H}" fill="url(#g)"/>
     </svg>`
  )

  /* The board card: the web app at the size the old hero used, inset from the
     top-left so the phone can overlap its bottom-right corner. */
  const board = await roundedShot(`shots/board/web.webp`, 1560, 18, 0.12)
  /* The phone: the iOS Agent composer (EXP-825) in a bezel, overlapping. */
  const phone = await roundedShot(`shots/chat/ios.webp`, 420, 44, 0.06)
  const bezel = 10
  const phoneFrame = await sharp({
    create: {
      width: phone.width + 2 * bezel,
      height: phone.height + 2 * bezel,
      channels: 4,
      background: { r: 0, g: 0, b: 0, alpha: 1 },
    },
  })
    .composite([
      {
        input: rounded(phone.width + 2 * bezel, phone.height + 2 * bezel, 54),
        blend: `dest-in`,
      },
      { input: phone.buffer, top: bezel, left: bezel },
      {
        input: hairline(
          phone.width + 2 * bezel,
          phone.height + 2 * bezel,
          54,
          0.14
        ),
        blend: `over`,
      },
    ])
    .png()
    .toBuffer()
  const phoneH = phone.height + 2 * bezel

  mkdirSync(dirname(OUT), { recursive: true })
  await sharp(background)
    .composite([
      { input: board.buffer, top: 34, left: 108 },
      {
        input: phoneFrame,
        top: Math.round(H - phoneH - 40),
        left: Math.round(W - phone.width - 2 * bezel - 150),
      },
    ])
    .webp({ quality: 88 })
    .toFile(OUT)
  console.log(`wrote ${OUT}`)
}

await main()
