import { describe, expect, it } from "vitest"
import icons from "../../icons/icons.json" with { type: "json" }
import fixture from "../fixtures/image-actions.json" with { type: "json" }
import {
  copyEncoding,
  imageMenu,
  imageSource,
  imageSourceAllowed,
  pressMenu,
  shareFileName,
  type ImageInput,
} from "./image-actions"

const hex = (h: string) => new Uint8Array((h.match(/../g) ?? []).map((b) => Number.parseInt(b, 16)))

describe(`imageMenu`, () => {
  for (const c of fixture.menuCases) {
    it(`${c.input} canShareFile=${c.canShareFile}: ${c.$why}`, () => {
      expect(imageMenu(c.input as ImageInput, c.canShareFile)).toEqual(c.expected)
    })
  }
})

describe(`pressMenu`, () => {
  for (const c of fixture.pressCases) {
    it(c.$why, () => {
      expect(pressMenu(c.onImage, c.imageReady, c.ownerMenu)).toBe(c.expected)
    })
  }
})

describe(`copyEncoding`, () => {
  for (const c of fixture.encodingCases) {
    it(c.$why, () => {
      expect(copyEncoding(hex(c.head))).toBe(c.expected)
    })
  }
})

describe(`shareFileName`, () => {
  for (const c of fixture.shareNameCases) {
    it(`${JSON.stringify(c.name)}: ${c.$why}`, () => {
      expect(shareFileName(c.name)).toBe(c.expected)
    })
  }
})

describe(`imageSource`, () => {
  const origin = fixture.sourcePolicy.origin
  for (const c of fixture.sourceCases) {
    it(`${JSON.stringify(c.src)}: ${c.$why}`, () => {
      expect(imageSource(c.src, c.alt, origin)).toEqual(c.expected)
      expect(imageSourceAllowed(c.src, origin)).toBe(c.expected.kind === `load`)
    })
  }

  it(`a trailing slash on the origin changes nothing`, () => {
    expect(imageSource(`${origin}/api/attachments/0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90`, ``, `${origin}/`)).toEqual({ kind: `load` })
  })

  it(`no origin: only relative attachments are ours`, () => {
    expect(imageSourceAllowed(`${origin}/api/attachments/0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90`, ``)).toBe(false)
    expect(imageSourceAllowed(`/api/attachments/0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90`, ``)).toBe(true)
  })
})

describe(`the fixture`, () => {
  it(`reuses existing concept icons only`, () => {
    const concepts = icons.semantic as Record<string, string>
    for (const concept of Object.values(fixture.icons)) expect(concepts[concept]).toBeDefined()
  })

  it(`the views in and out never overlap`, () => {
    const out = new Set(fixture.views.out)
    for (const v of fixture.views.in) expect(out.has(v)).toBe(false)
  })

  it(`every menu item has copy, an icon and a testId`, () => {
    for (const items of Object.values(fixture.menus)) {
      for (const item of items) {
        expect(fixture.copy[item as keyof typeof fixture.copy]).toBeTruthy()
        expect(fixture.icons[item as keyof typeof fixture.icons]).toBeTruthy()
        expect(fixture.testIds[item as keyof typeof fixture.testIds]).toBeTruthy()
      }
    }
  })
})
