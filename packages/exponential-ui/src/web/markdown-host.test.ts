import { describe, expect, test } from "bun:test"

import fixture from "../../fixtures/markdown-host.json" with { type: "json" }
import host from "../../catalog/host.json" with { type: "json" }
import type { AnyHostPlugin, TextLinkSegment } from "./host-types"
import { MARKDOWN_IMAGE_MAX_HEIGHT, markdownImageBox, markdownImageCap, markdownImageSizeOf, markdownImageStyle, parseMarkdown } from "./markdown"
import { normalMarkdown, type NormalBlock, type NormalRun } from "./markdown-normal"
import { linkHref } from "./urls"
import { linkedMarkdown } from "./markdown-linked"

// #VAPP-258 (catalog/host.json `markdown`): the host hooks a Markdown asks,
// the web side (React and Solid share `linkedMarkdown`). The fake host is
// the fixture's; links compare AS PAINTED (through the URL policy).

type Link = { href: string; chip?: TextLinkSegment[`chip`] }
const links = fixture.links as Record<string, Link>
const keys = Object.keys(links)

function textLinks(text: string): TextLinkSegment[] | null {
  const out: TextLinkSegment[] = []
  let plain = ``
  let found = false
  for (let i = 0; i < text.length; ) {
    const key = keys.find((k) => text.startsWith(k, i))
    if (!key) {
      plain += text[i++]
      continue
    }
    if (plain) out.push({ text: plain })
    plain = ``
    out.push({ text: key, ...links[key] })
    found = true
    i += key.length
  }
  if (plain) out.push({ text: plain })
  return found ? out : null
}

const fake: AnyHostPlugin = {
  urls: fixture.policy,
  textLinks,
  imageSize: (src) => (fixture.images as Record<string, { width: number; height: number }>)[src] ?? null,
  attachment: (href) => (fixture.attachments as Record<string, { kind: `video` | `audio` | `file`; name: string }>)[href] ?? null,
}

function painted(blocks: NormalBlock[]): NormalBlock[] {
  const police = (runs: NormalRun[]): NormalRun[] =>
    runs.map((run) => {
      if (!run.link) return run
      const href = linkHref(fake, run.link)
      if (href) return { ...run, link: href }
      const { link: _, chip: __, ...rest } = run
      return rest
    })
  return blocks.map((b) => {
    if (b.kind === `table`) return { ...b, header: b.header.map(police), rows: b.rows.map((r) => r.map(police)) }
    return `runs` in b ? { ...b, runs: police(b.runs) } : b
  })
}

describe(`markdown-host.json (web)`, () => {
  test(`the contract's max height`, () => {
    expect(MARKDOWN_IMAGE_MAX_HEIGHT).toBe(host.markdown.imageMaxHeight)
  })
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(painted(normalMarkdown(linkedMarkdown(fake, parseMarkdown(entry.text)) as never))).toEqual(entry.blocks as never)
    })
  }
  for (const entry of fixture.imageBoxes) {
    test(`imageBoxes: ${entry.name}`, () => {
      const e = entry as typeof entry & { maxHeight?: number | null; viewport?: number; loaded?: { width: number; height: number } }
      const box = markdownImageBox(markdownImageSizeOf(entry.size, e.loaded), entry.available, e.maxHeight === undefined ? undefined : e.maxHeight, e.viewport)
      expect(box.width).toBeCloseTo(entry.box.width, 2)
      expect(box.height).toBeCloseTo(entry.box.height, 2)
    })
  }
  test(`imageMaxHeight: unset or invalid = the catalog's cap, null = none`, () => {
    expect(markdownImageCap()).toBe(MARKDOWN_IMAGE_MAX_HEIGHT)
    expect(markdownImageCap({ imageMaxHeight: 240 })).toBe(240)
    expect(markdownImageCap({ imageMaxHeight: -1 })).toBe(MARKDOWN_IMAGE_MAX_HEIGHT)
    expect(markdownImageCap({ imageMaxHeight: null })).toBeNull()
    expect(host.markdown.options.imageMaxHeight.default).toBe(MARKDOWN_IMAGE_MAX_HEIGHT)
    expect(markdownImageStyle(undefined, undefined, MARKDOWN_IMAGE_MAX_HEIGHT)).toBeUndefined()
    expect(markdownImageStyle(1600, 900, null)?.[`aspect-ratio`]).toBe(`1600 / 900`)
  })
  test(`no hooks = the tree as parsed`, () => {
    for (const entry of fixture.cases) {
      const tree = parseMarkdown(entry.text)
      expect(linkedMarkdown({ urls: fixture.policy }, tree)).toBe(tree)
    }
  })
})
