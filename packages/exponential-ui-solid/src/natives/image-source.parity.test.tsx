// #EXP-1945 (host.imageSource, web/image-actions.ts): a picture the host
// refuses never loads: Image / ZoomableImage / a Markdown image become THE
// link (or the alt as text) in their place, the same DOM on React and Solid.

import { describe, expect, it } from "vitest"
import type { NestedNode } from "@exponential_app/eui"
import type { ImageSourceVerdict } from "@exponential_app/eui/web"
import { expectDomParity, renderReact } from "../test/parity"

const imageSource = (src: string, alt: string): ImageSourceVerdict =>
  src.startsWith(`https://example.com/`) ? { kind: `link`, href: src, text: alt.trim() || `Image: example.com` } : src.startsWith(`/uploads/`) ? { kind: `text`, text: alt.trim() } : { kind: `load` }

const tree = {
  id: `root`,
  component: `Stack`,
  children: [
    { id: `i1`, component: `Image`, props: { src: `https://example.com/a.png`, alt: `A cat` } },
    { id: `i2`, component: `Image`, props: { src: `https://example.com/b.png`, alt: `` } },
    { id: `i3`, component: `Image`, props: { src: `/uploads/c.png`, alt: `Plain` } },
    { id: `i4`, component: `Image`, props: { src: `/uploads/d.png`, alt: `` } },
    { id: `i5`, component: `Image`, props: { src: `data:image/png;base64,iVBORw0KGgo=`, alt: `Loads` } },
    { id: `z1`, component: `ZoomableImage`, props: { src: `https://example.com/z.png`, alt: `Zoom` } },
    { id: `m1`, component: `Markdown`, props: { text: `Before\n\n![Shot](https://example.com/m.png)\n\n![](/uploads/x.png)\n\nAfter` } },
  ],
} as unknown as NestedNode

const host = { imageSource }

describe(`#EXP-1945: imageSource`, { timeout: 60_000 }, () => {
  it(`React and Solid draw the same stand-ins`, async () => expectDomParity({ tree, reactHost: host, solidHost: host }))

  it(`a refused picture is a link (no <img>, nothing fetched), else its alt`, async () => {
    const r = await renderReact({ tree, reactHost: host })
    try {
      const imgs = [...r.root.querySelectorAll(`img`)].map((img) => img.getAttribute(`src`))
      expect(imgs).toEqual([`data:image/png;base64,iVBORw0KGgo=`])
      const links = [...r.root.querySelectorAll(`a`)].map((a) => [a.getAttribute(`href`), a.textContent])
      expect(links).toEqual([
        [`https://example.com/a.png`, `A cat`],
        [`https://example.com/b.png`, `Image: example.com`],
        [`https://example.com/z.png`, `Zoom`],
        [`https://example.com/m.png`, `Shot`],
      ])
      expect(r.root.textContent).toContain(`Plain`)
    } finally {
      r.cleanup()
    }
  })
})
