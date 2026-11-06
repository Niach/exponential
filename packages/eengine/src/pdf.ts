// VAPP-265: `print.pdf`, the Engine's own tool. ONE layout and ONE writer both
// twins produce byte for byte (capabilities.json `pdf`, `pdfCases`: sha256 + size
// locked). Pure and browser-safe; the arguments arrive validated.

import fixture from "../fixtures/capabilities.json" with { type: "json" }

export type PdfBlock = { heading: string } | { text: string } | { image: string; caption?: string } | { pageBreak: true }

export interface PdfArguments {
  title: string
  name?: string
  blocks: readonly PdfBlock[]
}

export type PdfResult = { bytes: Uint8Array; pages: number } | { error: string }

const [PAGE_W, PAGE_H] = fixture.pdf.page as [number, number]
const MARGIN = fixture.pdf.margin
const TOP = PAGE_H - MARGIN
const WIDTH = PAGE_W - 2 * MARGIN
const IMAGE_MAX_H = TOP - MARGIN
const WIDTHS: Record<`F1` | `F2`, readonly number[]> = { F1: fixture.pdf.widths.Helvetica, F2: fixture.pdf.widths[`Helvetica-Bold`] }
const WIN_ANSI = new Map(Object.entries(fixture.pdf.winAnsi as Record<string, number>).map(([cp, code]) => [Number(cp), code]))

/** A code point → its WinAnsi byte; null = dropped (a control). */
function winAnsi(cp: number): number | null {
  if (cp === 9) return 0x20
  if ((cp >= 0x20 && cp <= 0x7e) || (cp >= 0xa0 && cp <= 0xff)) return cp
  if (cp < 0x20 || (cp >= 0x7f && cp <= 0x9f)) return null
  return WIN_ANSI.get(cp) ?? 0x3f
}

const encode = (s: string): number[] => [...s].map((c) => winAnsi(c.codePointAt(0) as number)).filter((b): b is number => b !== null)
const widthSum = (font: `F1` | `F2`, bytes: readonly number[]) => bytes.reduce((sum, b) => sum + WIDTHS[font][b], 0)
const fits = (font: `F1` | `F2`, size: number, bytes: readonly number[]) => widthSum(font, bytes) * size <= WIDTH * 1000

/** A paragraph per `\n`, words on runs of spaces, greedy; a too-wide word breaks at its longest fitting prefix. */
export function wrapLines(text: string, font: `F1` | `F2`, size: number): number[][] {
  const lines: number[][] = []
  for (const paragraph of text.split(`\n`)) {
    const words: number[][] = []
    let word: number[] = []
    for (const b of encode(paragraph)) {
      if (b === 0x20) {
        if (word.length) words.push(word)
        word = []
      } else word.push(b)
    }
    if (word.length) words.push(word)
    if (!words.length) {
      lines.push([])
      continue
    }
    let line: number[] = []
    for (let w of words) {
      if (line.length) {
        const joined = [...line, 0x20, ...w]
        if (fits(font, size, joined)) {
          line = joined
          continue
        }
        lines.push(line)
        line = []
      }
      while (!fits(font, size, w)) {
        let n = 1
        while (n < w.length && fits(font, size, w.slice(0, n + 1))) n++
        lines.push(w.slice(0, n))
        w = w.slice(n)
      }
      line = w
    }
    lines.push(line)
  }
  return lines
}

/** A number rounded to 2 decimals, shortest form (`12`, `12.5`, `-0.25`). */
export function pdfNumber(x: number): string {
  const r = Math.sign(x) * Math.round(Math.abs(x) * 100)
  if (r === 0) return `0`
  const abs = Math.abs(r)
  const whole = Math.floor(abs / 100)
  const frac = abs % 100
  const sign = r < 0 ? `-` : ``
  if (frac === 0) return `${sign}${whole}`
  const digits = String(frac).padStart(2, `0`).replace(/0$/, ``)
  return `${sign}${whole}.${digits}`
}

function pdfString(bytes: readonly number[]): string {
  let out = `(`
  for (const b of bytes) {
    if (b === 0x28 || b === 0x29 || b === 0x5c) out += `\\${String.fromCharCode(b)}`
    else if (b >= 0x80) out += `\\${b.toString(8).padStart(3, `0`)}`
    else out += String.fromCharCode(b)
  }
  return `${out})`
}

interface PdfImage {
  width: number
  height: number
  dict: string
  data: Uint8Array
}

const u16 = (b: Uint8Array, i: number) => (b[i] << 8) | b[i + 1]
const u32 = (b: Uint8Array, i: number) => ((b[i] << 24) >>> 0) + ((b[i + 1] << 16) | (b[i + 2] << 8) | b[i + 3])
const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]

function jpegImage(b: Uint8Array): PdfImage | null {
  let i = 2
  while (i + 4 <= b.length) {
    if (b[i] !== 0xff) return null
    const marker = b[i + 1]
    if (marker === 0xff) {
      i++
      continue
    }
    if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd8)) {
      i += 2
      continue
    }
    if (marker === 0xd9 || marker === 0xda) return null
    const length = u16(b, i + 2)
    if (length < 2) return null
    if (marker >= 0xc0 && marker <= 0xc3) {
      if (i + 10 > b.length) return null
      const height = u16(b, i + 5)
      const width = u16(b, i + 7)
      const components = b[i + 9]
      const space = components === 1 ? `DeviceGray` : components === 3 ? `DeviceRGB` : components === 4 ? `DeviceCMYK` : null
      if (!space || !width || !height) return null
      return { width, height, dict: `/Type /XObject /Subtype /Image /Width ${width} /Height ${height} /ColorSpace /${space} /BitsPerComponent 8 /Filter /DCTDecode`, data: b }
    }
    i += 2 + length
  }
  return null
}

function pngImage(b: Uint8Array): PdfImage | null {
  let i = 8
  let header: { width: number; height: number; colors: number } | null = null
  const idat: Uint8Array[] = []
  while (i + 8 <= b.length) {
    const length = u32(b, i)
    const type = String.fromCharCode(b[i + 4], b[i + 5], b[i + 6], b[i + 7])
    const data = b.subarray(i + 8, i + 8 + length)
    if (data.length < length) return null
    if (!header) {
      if (type !== `IHDR` || length < 13) return null
      const width = u32(data, 0)
      const height = u32(data, 4)
      const [depth, colorType, compression, filter, interlace] = data.subarray(8, 13)
      if (depth !== 8 || (colorType !== 0 && colorType !== 2) || compression !== 0 || filter !== 0 || interlace !== 0 || !width || !height) return null
      header = { width, height, colors: colorType === 0 ? 1 : 3 }
    } else if (type === `IDAT`) idat.push(data)
    else if (type === `IEND`) break
    i += 12 + length
  }
  if (!header || !idat.length) return null
  const joined = new Uint8Array(idat.reduce((n, d) => n + d.length, 0))
  let o = 0
  for (const d of idat) {
    joined.set(d, o)
    o += d.length
  }
  const { width, height, colors } = header
  const space = colors === 1 ? `DeviceGray` : `DeviceRGB`
  return {
    width,
    height,
    dict: `/Type /XObject /Subtype /Image /Width ${width} /Height ${height} /ColorSpace /${space} /BitsPerComponent 8 /Filter /FlateDecode /DecodeParms << /Predictor 15 /Colors ${colors} /BitsPerComponent 8 /Columns ${width} >>`,
    data: joined,
  }
}

/** A JPEG or a supported PNG → its XObject, else null. */
export function pdfImage(bytes: Uint8Array): PdfImage | null {
  if (bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) return jpegImage(bytes)
  if (PNG_SIGNATURE.every((v, k) => bytes[k] === v)) return pngImage(bytes)
  return null
}

interface Page {
  ops: string[]
  images: number[]
}

const utf16Hex = (s: string): string => {
  let out = `FEFF`
  for (let i = 0; i < s.length; i++) out += s.charCodeAt(i).toString(16).toUpperCase().padStart(4, `0`)
  return out
}

/** Lays out and writes the document. `getBlob` resolves an app file id. */
export async function renderPdf(args: PdfArguments, getBlob: (id: string) => Promise<Uint8Array | null>): Promise<PdfResult> {
  const pages: Page[] = [{ ops: [], images: [] }]
  const images: { id: string; image: PdfImage }[] = []
  let y = TOP
  const page = () => pages[pages.length - 1]
  const empty = () => y === TOP
  const newPage = () => {
    pages.push({ ops: [], images: [] })
    y = TOP
  }
  const lines = (text: string, font: `F1` | `F2`, size: number, leading: number) => {
    for (const line of wrapLines(text, font, size)) {
      if (!empty() && y - leading < MARGIN) newPage()
      if (line.length) page().ops.push(`BT /${font} ${size} Tf ${MARGIN} ${pdfNumber(y - size)} Td ${pdfString(line)} Tj ET`)
      y -= leading
    }
  }
  for (const block of args.blocks) {
    if (`heading` in block) {
      if (!empty()) y -= 12
      lines(block.heading, `F2`, 18, 24)
      y -= 4
    } else if (`text` in block) {
      lines(block.text, `F1`, 11, 15)
      y -= 6
    } else if (`image` in block) {
      let k = images.findIndex((i) => i.id === block.image)
      if (k < 0) {
        const bytes = await getBlob(block.image)
        if (!bytes) return { error: `notFound: ${block.image}` }
        const image = pdfImage(bytes)
        if (!image) return { error: `unsupportedImage: ${block.image}` }
        images.push({ id: block.image, image })
        k = images.length - 1
      }
      const { width, height } = images[k].image
      const scale = Math.min(WIDTH / width, IMAGE_MAX_H / height, 1)
      const w = width * scale
      const h = height * scale
      if (!empty() && y - h < MARGIN) newPage()
      page().ops.push(`q ${pdfNumber(w)} 0 0 ${pdfNumber(h)} ${MARGIN} ${pdfNumber(y - h)} cm /Im${k + 1} Do Q`)
      if (!page().images.includes(k + 1)) page().images.push(k + 1)
      y -= h
      if (block.caption !== undefined) {
        y -= 4
        lines(block.caption, `F1`, 9, 12)
      }
      y -= 6
    } else if (!empty()) newPage()
  }
  if (pages.length > 1 && empty()) pages.pop()

  // The file.
  const chunks: Uint8Array[] = []
  let offset = 0
  const offsets: number[] = []
  const latin = (s: string) => Uint8Array.from(s, (c) => c.charCodeAt(0))
  const push = (bytes: Uint8Array) => {
    chunks.push(bytes)
    offset += bytes.length
  }
  const object = (body: string, stream?: Uint8Array) => {
    offsets.push(offset)
    const n = offsets.length
    if (stream) {
      push(latin(`${n} 0 obj\n<< ${body}${body ? ` ` : ``}/Length ${stream.length} >>\nstream\n`))
      push(stream)
      push(latin(`\nendstream\nendobj\n`))
    } else push(latin(`${n} 0 obj\n${body}\nendobj\n`))
  }
  push(Uint8Array.from([0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x34, 0x0a, 0x25, 0xe2, 0xe3, 0xcf, 0xd3, 0x0a]))
  const firstPage = 6 + images.length
  const kids = pages.map((_, i) => `${firstPage + 2 * i} 0 R`).join(` `)
  object(`<< /Type /Catalog /Pages 2 0 R >>`)
  object(`<< /Type /Pages /Kids [${kids}] /Count ${pages.length} >>`)
  object(`<< /Type /Font /Subtype /Type1 /BaseFont /${fixture.pdf.fonts.F1} /Encoding /WinAnsiEncoding >>`)
  object(`<< /Type /Font /Subtype /Type1 /BaseFont /${fixture.pdf.fonts.F2} /Encoding /WinAnsiEncoding >>`)
  object(`<< /Title <${utf16Hex(args.title)}> /Producer (Exponential Engine) >>`)
  for (const { image } of images) object(image.dict, image.data)
  pages.forEach((p, i) => {
    const xobjects = [...p.images].sort((a, b) => a - b).map((k) => `/Im${k} ${5 + k} 0 R`)
    const resources = `<< /Font << /F1 3 0 R /F2 4 0 R >>${xobjects.length ? ` /XObject << ${xobjects.join(` `)} >>` : ``} >>`
    object(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${PAGE_W} ${PAGE_H}] /Resources ${resources} /Contents ${firstPage + 2 * i + 1} 0 R >>`)
    object(``, latin(p.ops.join(`\n`)))
  })
  const xref = offset
  const size = offsets.length + 1
  push(latin(`xref\n0 ${size}\n0000000000 65535 f \n${offsets.map((o) => `${String(o).padStart(10, `0`)} 00000 n \n`).join(``)}`))
  push(latin(`trailer\n<< /Size ${size} /Root 1 0 R /Info 5 0 R >>\nstartxref\n${xref}\n%%EOF\n`))
  const bytes = new Uint8Array(offset)
  let at = 0
  for (const c of chunks) {
    bytes.set(c, at)
    at += c.length
  }
  return { bytes, pages: pages.length }
}
