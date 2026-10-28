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
