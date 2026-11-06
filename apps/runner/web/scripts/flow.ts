#!/usr/bin/env bun
// VAPP-271: the Photobook flow end to end on an `exponential bundle --target web`
// export, in Chromium at the web (1280×800) and web-mobile (390×844) sizes:
// a new album → photos through the platform picker → pages with thumbnails →
// Export PDF → a real PDF in the app's OPFS files (and Save's download).
// Every request is recorded: one leaving the export's origin fails the run.
// The export is served as plain static files (Bun.serve in a child process).
//
//   bun run ./scripts/flow.ts -- --export /tmp/out/Photobook-1.0.0-web [--shots /tmp/shots]

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { parseArgs } from "node:util"
import { deflateSync } from "node:zlib"
import { chromium, type Page } from "playwright"

const { values } = parseArgs({ options: { export: { type: `string` }, shots: { type: `string` } } })
if (!values.export) throw new Error(`--export <dir> is required`)
const root = resolve(values.export)
const shots = values.shots ? resolve(values.shots) : null
if (shots) mkdirSync(shots, { recursive: true })

// ---- three small real PNGs (the picker's files) ----
const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
  return c >>> 0
})
const crc32 = (buf: Buffer) => {
  let c = 0xffffffff
  for (const b of buf) c = crcTable[(c ^ b) & 0xff]! ^ (c >>> 8)
  return (c ^ 0xffffffff) >>> 0
}
const chunk = (type: string, data: Buffer) => {
  const len = Buffer.alloc(4)
  len.writeUInt32BE(data.length)
  const body = Buffer.concat([Buffer.from(type, `ascii`), data])
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(crc32(body))
  return Buffer.concat([len, body, crc])
}
const png = (w: number, h: number, [r, g, b]: [number, number, number]) => {
  const raw = Buffer.alloc((w * 3 + 1) * h)
  for (let y = 0; y < h; y++) {
    raw[y * (w * 3 + 1)] = 0
    for (let x = 0; x < w; x++) {
      const o = y * (w * 3 + 1) + 1 + x * 3
      const t = (x + y) / (w + h)
      raw[o] = Math.round(r * (1 - t) + 255 * t * 0.3)
      raw[o + 1] = Math.round(g * (1 - t * 0.5))
      raw[o + 2] = Math.round(b * (0.5 + t * 0.5))
    }
  }
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(w, 0)
  ihdr.writeUInt32BE(h, 4)
  ihdr.set([8, 2, 0, 0, 0], 8)
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk(`IHDR`, ihdr), chunk(`IDAT`, deflateSync(raw)), chunk(`IEND`, Buffer.alloc(0))])
}
const photoDir = mkdtempSync(join(tmpdir(), `photobook-photos-`))
const photos = ([[`lake.png`, [40, 110, 220]], [`forest.png`, [30, 160, 70]], [`sunset.png`, [230, 120, 40]]] as const).map(([name, color]) => {
  const path = join(photoDir, name)
  writeFileSync(path, png(320, 240, [...color]))
  return path
})

// ---- the export, served as static files ----
const port = 4900 + Math.floor(Math.random() * 500)
const origin = `http://127.0.0.1:${port}`
const server = Bun.spawn(
  [`bun`, `-e`, `const r=process.argv[1];Bun.serve({hostname:"127.0.0.1",port:${port},async fetch(q){let p=decodeURIComponent(new URL(q.url).pathname);if(p.endsWith("/"))p+="index.html";const f=Bun.file(r+p);return (await f.exists())?new Response(f):new Response("not found",{status:404})}})`, root],
  { stdout: `ignore`, stderr: `inherit` },
)
for (let i = 0; i < 100; i++) {
  if (await fetch(`${origin}/`).then((r) => r.ok, () => false)) break
  await Bun.sleep(100)
}

/** The app's OPFS files whose bytes start `%PDF-`: [path, size, head, tail]. */
const opfsPdfs = (page: Page) =>
  page.evaluate(async () => {
    const out: { path: string; size: number; head: string; tail: string }[] = []
    const walk = async (dir: FileSystemDirectoryHandle, at: string) => {
      for await (const [name, handle] of (dir as unknown as { entries(): AsyncIterable<[string, FileSystemHandle]> }).entries()) {
        if (handle.kind === `directory`) await walk(handle as FileSystemDirectoryHandle, `${at}/${name}`)
        else {
          const bytes = new Uint8Array(await (await (handle as FileSystemFileHandle).getFile()).arrayBuffer())
          const text = (b: Uint8Array) => String.fromCharCode(...b)
          if (text(bytes.slice(0, 5)) === `%PDF-`) out.push({ path: `${at}/${name}`, size: bytes.length, head: text(bytes.slice(0, 8)), tail: text(bytes.slice(-16)) })
        }
      }
    }
    await walk(await navigator.storage.getDirectory(), ``)
    return out
  })

const shot = async (page: Page, factor: string, name: string) => {
  if (!shots) return
  // The page scrolls inside the shell's Box (never the window): back to its top.
  await page.evaluate(() => document.querySelectorAll(`*`).forEach((e) => e.scrollTop > 0 && (e.scrollTop = 0)))
  await page.waitForTimeout(600)
  await page.screenshot({ path: join(shots, `${factor === `web` ? `web` : `web-mobile`}-${name}.png`) })
}

const browser = await chromium.launch()
let failed = false
for (const [factor, viewport] of [[`web`, { width: 1280, height: 800 }], [`web-mobile`, { width: 390, height: 844 }]] as const) {
  const context = await browser.newContext({ viewport, deviceScaleFactor: 2, isMobile: factor === `web-mobile`, hasTouch: factor === `web-mobile`, acceptDownloads: true })
  // Playwright's file chooser cannot answer Chromium's File System Access
  // pickers (CDP aborts showOpenFilePicker with no element to fill), so the
  // run takes the runner's other platform path, the one Chrome on Android,
  // Firefox and Safari take: the hidden <input type=file> and the <a download>.
  await context.addInitScript(() => {
    delete (window as { showOpenFilePicker?: unknown }).showOpenFilePicker
    delete (window as { showSaveFilePicker?: unknown }).showSaveFilePicker
  })
  const page = await context.newPage()
  const errors: string[] = []
  const requests: string[] = []
  const foreign: string[] = []
  const onRequest = (url: string) => {
    requests.push(url)
    if (!url.startsWith(`${origin}/`) && !url.startsWith(`blob:${origin}/`) && !url.startsWith(`data:`)) foreign.push(url)
  }
  context.on(`request`, (r) => onRequest(r.url()))
  context.on(`serviceworker`, (w) => w.on(`console`, () => {}))
  page.on(`pageerror`, (e) => errors.push(String(e)))
  page.on(`console`, (m) => m.type() === `error` && errors.push(m.text()))
  const tap = (locator: ReturnType<Page[`locator`]>) => (factor === `web` ? locator.click() : locator.tap())
  const step = (s: string) => console.log(`[${factor}] ${s}`)
  try {
    await page.goto(`${origin}/`)
    await page.getByText(`No albums yet`).waitFor({ timeout: 20_000 })
    step(`albums: empty`)
    await shot(page, factor, `albums-empty`)

    // The Permissions sheet (the shell's Row when wide, its icon Button when compact).
    if (factor === `web`) await tap(page.getByText(`Permissions`, { exact: true }).first())
    // runner.json's compact bar: an icon-only ghost Button, the shell's last button (it carries no label).
    else await tap(page.locator(`button`).last())
    await page.getByText(`Pick the photos for an album.`).waitFor({ timeout: 5000 })
    step(`permissions sheet open`)
    await shot(page, factor, `permissions`)
    await page.keyboard.press(`Escape`)
    await page.getByText(`Pick the photos for an album.`).waitFor({ state: `hidden`, timeout: 5000 })

    // A new album.
    await page.getByLabel(`New album`, { exact: true }).fill(`Summer 2026`)
    await tap(page.getByRole(`button`, { name: `New album` }))
    await tap(page.getByText(`Summer 2026`, { exact: true }).first())
    await page.getByText(`No pages yet`).waitFor({ timeout: 10_000 })
    step(`album open`)

    // Photos through the platform picker: the consent sheet asks first (photos.read).
    const addPage = async (files: string[], caption: string, asks: boolean) => {
      await page.getByLabel(`Caption`, { exact: true }).fill(caption)
      const chooser = page.waitForEvent(`filechooser`, { timeout: 10_000 })
      await tap(page.getByRole(`button`, { name: `Add photos` }).first())
      if (asks) {
        const consent = page.getByRole(`button`, { name: `Continue` })
        await consent.waitFor({ timeout: 5000 })
        step(`consent sheet: Allow → Continue`)
        await shot(page, factor, `consent`)
        await tap(consent)
      }
      await (await chooser).setFiles(files)
    }
    await addPage(photos.slice(0, 2), `At the lake`, true)
    await page.getByText(`At the lake`).waitFor({ timeout: 10_000 })
    await addPage(photos.slice(2), `Evening`, false)
    await page.getByText(`Evening`, { exact: true }).waitFor({ timeout: 10_000 })
    // Thumbnails: every page image decoded from an app-file: src.
    await page.waitForFunction(() => {
      const imgs = [...document.querySelectorAll(`img[alt="Photo"]`)] as HTMLImageElement[]
      return imgs.length >= 3 && imgs.every((i) => i.complete && i.naturalWidth > 0)
    }, undefined, { timeout: 10_000 })
    const thumbs = await page.locator(`img[alt="Photo"]`).evaluateAll((imgs) => (imgs as HTMLImageElement[]).map((i) => `${i.naturalWidth}x${i.naturalHeight}`))
    step(`pages: 2, thumbnails ${thumbs.join(` `)}`)
    await shot(page, factor, `album-pages`)

    // Export PDF → the PDF in the app's files, then Save (a download, or the save picker).
    await tap(page.getByRole(`button`, { name: `Export PDF` }))
    await page.getByText(`Summer 2026.pdf`).waitFor({ timeout: 20_000 })
    const pdfs = await opfsPdfs(page)
    if (pdfs.length === 0) throw new Error(`no PDF in OPFS`)
    for (const p of pdfs) {
      step(`OPFS ${p.path}: ${p.size} bytes, head ${JSON.stringify(p.head)}, tail ${JSON.stringify(p.tail)}`)
      if (!p.tail.includes(`%%EOF`) || p.size < 1000) throw new Error(`not a whole PDF: ${p.path}`)
    }
    const download = page.waitForEvent(`download`, { timeout: 5000 }).catch(() => null)
    await tap(page.getByRole(`button`, { name: `Save` }))
    const saved = await download
    if (saved) {
      const path = await saved.path()
      const bytes = Buffer.from(await Bun.file(path).arrayBuffer())
      const ok = bytes.subarray(0, 5).toString() === `%PDF-` && bytes.subarray(-16).toString().includes(`%%EOF`)
      const pages = (bytes.toString(`latin1`).match(/\/Type\s*\/Page[^s]/g) ?? []).length
      const images = (bytes.toString(`latin1`).match(/\/Subtype\s*\/Image/g) ?? []).length
      step(`Save: download ${saved.suggestedFilename()} ${bytes.length} bytes, ${pages} pages, ${images} images, ${ok ? `valid PDF` : `NOT A PDF`}`)
      if (shots) await saved.saveAs(join(shots, `${factor}-${saved.suggestedFilename()}`))
      if (!ok) failed = true
    } else step(`Save: no download event (the save picker path)`)

    await page.waitForTimeout(500)
    const sameOrigin = [...new Set(requests.map((u) => u.replace(origin, ``)))]
    step(`requests: ${requests.length} (${sameOrigin.length} distinct, all ${origin}): ${sameOrigin.join(` `)}`)
    if (foreign.length) throw new Error(`requests left the origin: ${foreign.join(` `)}`)
    const backend = await page.evaluate(() => (globalThis as { __runnerBackend?: string }).__runnerBackend ?? null)
    step(`PASS (records backend ${backend}${errors.length ? `; console errors: ${errors.join(` | `)}` : ``})`)
  } catch (error) {
    failed = true
    console.error(`[${factor}] FAIL: ${error instanceof Error ? error.message : error}${errors.length ? `\n  errors: ${errors.join(` | `)}` : ``}`)
    if (shots) await page.screenshot({ path: join(shots, `${factor}-failure.png`) }).catch(() => {})
  }
  await context.close()
}
await browser.close()
server.kill()
rmSync(photoDir, { recursive: true, force: true })
process.exit(failed ? 1 : 0)
