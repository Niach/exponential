import { describe, expect, it } from "vitest"
import { isStoryCard } from "@exp/domain-contract/stories"
import { buildSurfaceCard } from "@/lib/sessions/plan"

// S-3 stories.json: a surface card may carry screenshots and a story flag.
const surface = { root: { id: `t`, component: `Text`, props: { text: `Green` } } }

describe(`buildSurfaceCard (S-3 stories)`, () => {
  it(`a plain surface carries neither pictures nor a flag, and is no story`, () => {
    const card = buildSurfaceCard({ surface })
    expect(card).toEqual({ kind: `surface`, surface: { root: surface.root, data: null }, title: null })
    expect(isStoryCard(card)).toBe(false)
  })

  it(`pictures are declared unresolved; the card is a story once one resolves`, () => {
    const card = buildSurfaceCard({ surface, pictures: [{ label: `web chats`, caption: ` Ring ` }] })
    expect(card.pictures).toEqual([
      { label: `web chats`, attachmentId: null, width: null, height: null, caption: `Ring` },
    ])
    expect(isStoryCard(card)).toBe(false)
    expect(isStoryCard({ ...card, pictures: [{ ...card.pictures![0], attachmentId: `a1` }] })).toBe(true)
  })

  it(`an explicit story flag wins`, () => {
    expect(isStoryCard(buildSurfaceCard({ surface, story: true }))).toBe(true)
  })

  it(`a label declared twice is refused`, () => {
    expect(() => buildSurfaceCard({ surface, pictures: [{ label: `a` }, { label: `a` }] })).toThrow(/declared twice/)
  })
})

// EXP-1604: a Motion's animation is validated by the engine (WASM).
describe(`buildSurfaceCard (Motion)`, () => {
  const motion = (props: Record<string, unknown>) => ({ root: { id: `m`, component: `Motion`, props } })
  const spinner = {
    size: [48, 48],
    duration: 1000,
    layers: [{ type: `shape`, shapes: [{ type: `ellipse`, size: [36, 36] }, { type: `stroke`, color: `$color.primary`, width: 4 }] }],
  }

  it(`accepts an inline dialect animation`, () => {
    expect(buildSurfaceCard({ surface: motion(spinner) }).kind).toBe(`surface`)
  })

  it(`refuses a dialect error with its path`, () => {
    expect(() => buildSurfaceCard({ surface: motion({ ...spinner, layers: [{ type: `shape`, shapes: [{ type: `blob` }] }] }) })).toThrow(/invalid Motion: m: shape-type at \/layers\/0\/shapes\/0\/type/)
  })

  it(`refuses expressions in inline Lottie`, () => {
    const lottie = { v: `5.7.0`, fr: 30, ip: 0, op: 30, w: 10, h: 10, layers: [{ ty: 4, ind: 1, ip: 0, op: 30, st: 0, ks: { r: { a: 0, k: 0, x: `time` } }, shapes: [] }] }
    expect(() => buildSurfaceCard({ surface: motion({ lottie }) })).toThrow(/expression/)
  })

  it(`takes src only as an attachment`, () => {
    expect(() => buildSurfaceCard({ surface: motion({ src: `https://lottie.host/a.json` }) })).toThrow(/attachment/)
  })
})
