// #VAPP-258 (catalog/host.json `markdown`, fixtures/markdown-host.json): the
// host hooks a Markdown asks, painted. A chip = the Chip recipe inline (a
// tap opens through the host); a sized image keeps its aspect; a block image
// press calls `onImagePress`; a classified one-link paragraph is the Video /
// AudioPlayer native or the FileUpload file row. No hooks = as before.

import { afterEach, describe, expect, it, vi } from "vitest"
import { cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"
import type { HostPlugin } from "./host"

afterEach(cleanup)

const A = `https://app.example/api/attachments/`
const paint = (text: string, host?: HostPlugin) =>
  render(<ExponentialSurface root={reduceNested({ id: `md`, component: `Markdown`, props: { text } }, { catalogId: CORE_CATALOG_ID }).root} theme="neutral" id="hooks" host={host} />).container

const chipHost = (openUrl = vi.fn()): HostPlugin => ({
  openUrl,
  textLinks: (text) => {
    const at = text.indexOf(`#ABC-1`)
    if (at < 0) return null
    return [{ text: text.slice(0, at) }, { text: `#ABC-1`, href: `https://app.example/i/ABC-1`, chip: { kind: `issue`, label: `ABC-1 Fix login`, icon: `ui-issue` } }, { text: text.slice(at + 6) }]
  },
})

describe(`#VAPP-258 Markdown host hooks`, () => {
  it(`a chip segment paints the Chip recipe inline and opens through the host`, () => {
    const openUrl = vi.fn()
    const c = paint(`see #ABC-1 now`, chipHost(openUrl))
    const chip = c.querySelector(`.xui-Markdown-paragraph > a.xui-md-chip`)!
    expect(chip).not.toBeNull()
    expect(chip.classList.contains(`xui-Chip-root`)).toBe(true)
    const scope = c.querySelector(`.xui-surface`)!.className.split(` `).find((k) => k.startsWith(`xui-t-`))!
    expect(chip.classList.contains(scope)).toBe(true)
    expect(chip.getAttribute(`data-chip`)).toBe(`issue`)
    expect(chip.getAttribute(`href`)).toBe(`https://app.example/i/ABC-1`)
    expect(chip.querySelector(`.xui-Chip-label`)!.textContent).toBe(`ABC-1 Fix login`)
    expect(chip.querySelector(`svg`)).not.toBeNull()
    expect(c.querySelector(`.xui-Markdown-paragraph`)!.textContent).toBe(`see ABC-1 Fix login now`)
    fireEvent.click(chip)
    expect(openUrl).toHaveBeenCalledWith(`https://app.example/i/ABC-1`)
  })

  it(`a chip with a denied href is text; an unknown kind is a plain link`, () => {
    const c = paint(`see #ABC-1`, { textLinks: (t) => (t.includes(`#ABC-1`) ? [{ text: `see ` }, { text: `#ABC-1`, href: `javascript:alert(1)`, chip: { kind: `issue`, label: `x` } }] : null) })
    expect(c.querySelector(`.xui-md-chip`)).toBeNull()
    expect(c.querySelector(`a`)).toBeNull()
    cleanup()
    const d = paint(`see #ABC-1`, { textLinks: (t) => (t.includes(`#ABC-1`) ? [{ text: `see ` }, { text: `#ABC-1`, href: `https://app.example/i/ABC-1`, chip: { kind: `widget` as never, label: `x` } }] : null) })
    expect(d.querySelector(`.xui-md-chip`)).toBeNull()
    expect(d.querySelector(`a.xui-Markdown-link`)!.textContent).toBe(`#ABC-1`)
  })

  it(`#FEED-208: an image in a paragraph's text is a block image between its text, every url form`, () => {
    const c = paint(`rel: ![rel](zim/x/y.jpg) root ![root](/zim/x/y.jpg)\n![abs](https://upload.example/p.png) tail`)
    const md = c.querySelector(`.xui-md`)!
    expect([...md.children].map((e) => e.getAttribute(`data-xui-part`))).toEqual([`Markdown/paragraph`, `Markdown/image`, `Markdown/paragraph`, `Markdown/image`, `Markdown/image`, `Markdown/paragraph`])
    expect([...md.querySelectorAll(`.xui-Markdown-paragraph`)].map((p) => p.textContent)).toEqual([`rel:`, `root`, `tail`])
    expect([...md.querySelectorAll(`.xui-Markdown-image`)].map((e) => e.getAttribute(`aria-label`))).toEqual([`rel`, `root`, `abs`])
    expect(md.querySelectorAll(`img`)).toHaveLength(3)
    expect(md.querySelectorAll(`img`)[2]!.getAttribute(`src`)).toBe(`https://upload.example/p.png`)
  })

  it(`a sized block image keeps its aspect, capped at 160px; unsized = the fixed box`, () => {
    const c = paint(`![wide](${A}wide.png)`, { imageSize: (src) => (src === `${A}wide.png` ? { width: 800, height: 200 } : null) })
    const box = c.querySelector<HTMLElement>(`.xui-Markdown-image`)!
    expect(box.style.aspectRatio).toBe(`800 / 200`)
    expect(box.style.width).toBe(`640px`)
    expect(box.style.maxWidth).toBe(`100%`)
    expect(box.getAttribute(`role`)).toBe(`img`)
    cleanup()
    const d = paint(`![x](${A}unknown.png)`, { imageSize: () => null })
    expect(d.querySelector<HTMLElement>(`.xui-Markdown-image`)!.style.aspectRatio).toBe(``)
  })

  it(`onImagePress: a press on a block image calls it with the src as written`, () => {
    const onImagePress = vi.fn()
    const c = paint(`![x](${A}a.png)`, { onImagePress })
    const box = c.querySelector(`.xui-Markdown-image`)!
    expect(box.getAttribute(`role`)).toBe(`button`)
    fireEvent.click(box)
    fireEvent.keyDown(box, { key: `Enter` })
    expect(onImagePress.mock.calls).toEqual([[`${A}a.png`], [`${A}a.png`]])
  })

  it(`attachments: the Video and AudioPlayer natives and the FileUpload file row`, () => {
    const kinds: Record<string, { kind: `video` | `audio` | `file`; name: string }> = {
      [`${A}clip.mp4`]: { kind: `video`, name: `clip.mp4` },
      [`${A}voice.m4a`]: { kind: `audio`, name: `voice.m4a` },
      [`${A}report.pdf`]: { kind: `file`, name: `report.pdf` },
    }
    const openUrl = vi.fn()
    const c = paint(`[clip](${A}clip.mp4)\n\n[voice](${A}voice.m4a)\n\n[the report](${A}report.pdf)\n\nsee [the report](${A}report.pdf)`, { attachment: (href) => kinds[href] ?? null, openUrl })
    const blocks = [...c.querySelectorAll(`.xui-Markdown-attachment`)]
    expect(blocks.map((b) => b.getAttribute(`data-attachment`))).toEqual([`video`, `audio`, `file`])
    expect(blocks[0]!.querySelector(`[data-xui-c="Video"]`)).not.toBeNull()
    expect(blocks[1]!.querySelector(`[data-xui-c="AudioPlayer"]`)!.textContent).toContain(`voice.m4a`)
    const row = blocks[2]!.querySelector(`.xui-FileUpload-file`)!
    expect(row.querySelector(`.xui-FileUpload-fileName`)!.textContent).toBe(`report.pdf`)
    fireEvent.click(row.querySelector(`a`)!)
    expect(openUrl).toHaveBeenCalledWith(`${A}report.pdf`)
    // Text around the link: still a paragraph.
    expect(c.querySelectorAll(`.xui-Markdown-paragraph`).length).toBe(1)
  })

  it(`no hooks: exactly as before`, () => {
    const c = paint(`see #ABC-1\n\n![x](${A}a.png)\n\n[clip](${A}clip.mp4)`)
    expect(c.querySelector(`.xui-md-chip, .xui-Markdown-attachment`)).toBeNull()
    expect(c.querySelector(`.xui-Markdown-image`)!.getAttribute(`role`)).toBe(`img`)
    expect(c.querySelector(`a.xui-Markdown-link`)!.textContent).toBe(`clip`)
  })
})
