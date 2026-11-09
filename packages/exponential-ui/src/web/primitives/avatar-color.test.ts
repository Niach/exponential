import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"

import { AVATAR_FILL_ALPHA, AVATAR_HUE_COUNT, AVATAR_PALETTE, avatarHueIndex, avatarHueKey, avatarInitials } from "./avatar-color"

// #EXP-1629: the Avatar fallback rule every renderer replays.
const fixture = JSON.parse(readFileSync(join(import.meta.dir, `../../../fixtures/avatar.json`), `utf8`)) as {
  palette: string[]
  fillAlpha: number
  keys: { case: string; id?: string; name?: string; email?: string; key: string | null; hue: number | null }[]
  hues: [string, number][]
  initials: [string, string][]
}
const design = JSON.parse(readFileSync(join(import.meta.dir, `../../../../design-tokens/tokens.json`), `utf8`)) as { avatar: Record<string, string> }

describe(`fixtures/avatar.json`, () => {
  test(`the palette is design-tokens avatar, in its order`, () => {
    const tokens = Object.entries(design.avatar).filter(([k]) => !k.startsWith(`$`)).map(([, v]) => v)
    expect(fixture.palette).toEqual(tokens)
    expect([...AVATAR_PALETTE] as string[]).toEqual(fixture.palette)
    expect(AVATAR_HUE_COUNT).toBe(fixture.palette.length)
    expect(AVATAR_FILL_ALPHA).toBe(fixture.fillAlpha)
  })

  test(`hues`, () => {
    for (const [seed, index] of fixture.hues) expect(avatarHueIndex(seed), seed).toBe(index)
  })

  test(`keys`, () => {
    for (const c of fixture.keys) {
      const key = avatarHueKey(c.id, c.name, c.email)
      expect(key ?? null, c.case).toBe(c.key)
      expect(key === undefined ? null : avatarHueIndex(key), c.case).toBe(c.hue)
    }
  })

  test(`initials`, () => {
    for (const [name, letters] of fixture.initials) expect(avatarInitials(name), name).toBe(letters)
  })
})
