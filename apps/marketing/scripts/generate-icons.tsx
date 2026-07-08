/* Dev-only favicon / app-icon generator. Produces the committed icon set in
   public/ (apple-touch-icon.png, icon-192.png, icon-512.png, favicon.ico).
   NOT part of the build — run it by hand when the mark changes:

     bun run scripts/generate-icons.tsx

   The brand mark (public/logo-light.svg — white circle mark) is composited onto
   a dark rounded-square background so it stays visible on light browser tabs,
   then rasterized at each size with @resvg/resvg-js. favicon.ico is assembled
   from 16/32/48px PNGs via png-to-ico. */

import { Resvg } from "@resvg/resvg-js"
import pngToIco from "png-to-ico"
import { readFileSync, writeFileSync } from "node:fs"
import { resolve, dirname } from "node:path"
import { fileURLToPath } from "node:url"

const HERE = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(HERE, `..`)
const PUBLIC = resolve(ROOT, `public`)

const BG = `#09090b`

/* Inline the mark paths onto a dark rounded background at the given canvas size.
   The mark itself is white; the paths come straight from logo-light.svg (a
   100×100 viewBox circle mask), scaled to ~64% and centered. */
function iconSvg(size: number): string {
  const radius = Math.round(size * 0.22)
  const markSize = Math.round(size * 0.64)
  const offset = Math.round((size - markSize) / 2)
  const mark = readFileSync(resolve(PUBLIC, `logo-light.svg`), `utf8`)
    /* strip the xml/svg wrapper attrs we don't want, keep inner content by
       re-wrapping in a positioned <svg> viewport */
    .replace(/<\?xml[^>]*\?>/, ``)
  /* Re-emit the mark as a nested <svg> so its 100×100 viewBox scales to markSize. */
  const inner = mark
    .replace(/width="[^"]*"/, `width="${markSize}"`)
    .replace(/height="[^"]*"/, `height="${markSize}"`)
    .replace(/<svg /, `<svg x="${offset}" y="${offset}" `)
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
  <rect width="${size}" height="${size}" rx="${radius}" ry="${radius}" fill="${BG}" />
  ${inner}
</svg>`
}

function renderPng(size: number): Buffer {
  return new Resvg(iconSvg(size), { fitTo: { mode: `width`, value: size } })
    .render()
    .asPng()
}

function main() {
  const targets: { file: string; size: number }[] = [
    { file: `apple-touch-icon.png`, size: 180 },
    { file: `icon-192.png`, size: 192 },
    { file: `icon-512.png`, size: 512 },
  ]
  for (const t of targets) {
    writeFileSync(resolve(PUBLIC, t.file), renderPng(t.size))
    console.log(`wrote public/${t.file} (${t.size}×${t.size})`)
  }

  return pngToIco([renderPng(16), renderPng(32), renderPng(48)]).then((ico) => {
    writeFileSync(resolve(PUBLIC, `favicon.ico`), ico)
    console.log(`wrote public/favicon.ico (16/32/48)`)
    console.log(`icons complete`)
  })
}

main()
