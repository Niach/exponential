// @vitest-environment node
import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1468 (sealed-content addendum 5): a SEALED story card's PR pictures
// are its clear `pictures` entries with an attachmentId, none when the clear
// `story` is false; captions never leave `sealed`.

const h = vi.hoisted(() => ({ results: [] as unknown[][] }))
vi.mock(`@/db/connection`, () => {
  const chain = (): Record<string, unknown> => {
    const rows = h.results.shift() ?? []
    const self: Record<string, unknown> = {
      from: () => self,
      where: () => self,
      orderBy: () => self,
      then: (resolve: (value: unknown) => unknown, reject?: (error: unknown) => unknown) => Promise.resolve(rows).then(resolve, reject),
    }
    return self
  }
  return { db: { select: vi.fn(() => chain()) } }
})
vi.mock(`@/lib/notification-email-policy`, () => ({
  appBaseUrl: () => `https://exp.example`,
  buildSessionDeepLinkPath: (slug: string, id: string) => `/t/${slug}/sessions/${id}`,
}))

const { loadStoryPictures, storyPicturesOfCard } = await import(`@/lib/pr-stories`)

const sealed = `v1:1:${`A`.repeat(80)}`
const root = `00000000-0000-4000-8000-000000000001`

const load = async (cards: unknown[]) => {
  h.results.push([{ id: root, parentId: null, branch: null, status: `idle` }], cards.map((card) => ({ card })))
  return (await loadStoryPictures(root, null)).map((p) => [p.label, p.attachmentId, p.url])
}

describe(`loadStoryPictures`, () => {
  beforeEach(() => {
    h.results.length = 0
  })

  it(`reads a plaintext story card as before (story:false skipped)`, async () => {
    expect(
      await load([
        { kind: `surface`, pictures: [{ label: `web list`, attachmentId: `a1`, caption: `List` }] },
        { kind: `surface`, story: false, pictures: [{ label: `web chat`, attachmentId: `a2` }] },
      ])
    ).toEqual([[`web list`, `a1`, `https://exp.example/api/attachments/a1`]])
  })

  it(`reads a sealed story card through its clear pictures (story true or absent)`, async () => {
    expect(
      await load([
        { kind: `surface`, story: true, pictures: [{ label: `web list`, attachmentId: `s1` }], sealed },
        { kind: `plan`, fresh: false, pictures: [{ label: `ios plan`, attachmentId: `s2` }], sealed },
      ])
    ).toEqual([
      [`web list`, `s1`, `https://exp.example/api/attachments/s1`],
      [`ios plan`, `s2`, `https://exp.example/api/attachments/s2`],
    ])
  })

  it(`skips a sealed story:false card and a picture still to upload`, async () => {
    expect(
      await load([
        { kind: `surface`, story: false, pictures: [{ label: `web chat`, attachmentId: `s3` }], sealed },
        { kind: `surface`, story: true, pictures: [{ label: `web later` }, { label: `ios now`, attachmentId: `s4` }], sealed },
      ])
    ).toEqual([[`ios now`, `s4`, `https://exp.example/api/attachments/s4`]])
  })
})

describe(`storyPicturesOfCard`, () => {
  it(`never carries sealed text: a sealed card yields label + id only`, () => {
    expect(storyPicturesOfCard({ kind: `surface`, pictures: [{ label: `web a`, attachmentId: `x`, caption: `leak` }], sealed })).toEqual([
      { label: `web a`, attachmentId: `x` },
    ])
  })

  it(`a sealed card of another kind or without pictures has none`, () => {
    expect(storyPicturesOfCard({ kind: `report`, pictures: [{ label: `web a`, attachmentId: `x` }], sealed })).toEqual([])
    expect(storyPicturesOfCard({ kind: `surface`, sealed })).toEqual([])
  })
})

describe(`validSealedCard: the clear story flag`, () => {
  it(`takes a boolean story beside sealed, refuses anything else`, async () => {
    const { validSealedCard } = await import(`@/lib/sessions/sealed`)
    expect(validSealedCard({ kind: `surface`, story: false, sealed })).toBe(true)
    expect(validSealedCard({ kind: `surface`, story: `no`, sealed })).toBe(false)
  })
})
