// EXP-698 r4 — the avatar hue contract.
//
// Four clients hash the same user id into the same palette slot, so a person
// keeps one colour everywhere. The fixture below is the contract: web, iOS,
// Android and desktop each pin these exact eight pairs. Changing a value here
// means recolouring existing users on one client only — don't.

import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"
import { AVATAR_HUE_COUNT, avatarHueIndex } from "./avatar-color"

const __dirname = dirname(fileURLToPath(import.meta.url))
const repoRoot = join(__dirname, `..`, `..`, `..`, `..`)

const tokens = JSON.parse(
  readFileSync(join(repoRoot, `packages/design-tokens/tokens.json`), `utf8`)
) as { avatar: Record<string, string> }

const FIXTURE: ReadonlyArray<readonly [string, number]> = [
  [``, 5],
  [`demo-mira`, 2],
  [`demo-jonas`, 4],
  [`demo-sofia`, 1],
  [`alex`, 5],
  [`7c9e6679-7425-40de-944b-e07fc1f90ae7`, 3],
  [`user_01HZY`, 1],
  [`ünïcödé`, 2],
]

describe(`avatarHueIndex`, () => {
  it(`matches the cross-client fixture`, () => {
    for (const [id, index] of FIXTURE) {
      expect(avatarHueIndex(id), id).toBe(index)
    }
  })

  it(`treats null and undefined as the empty id`, () => {
    expect(avatarHueIndex(null)).toBe(avatarHueIndex(``))
    expect(avatarHueIndex(undefined)).toBe(avatarHueIndex(``))
  })

  it(`stays inside the palette`, () => {
    for (let i = 0; i < 500; i++) {
      const index = avatarHueIndex(`user-${i}`)
      expect(Number.isInteger(index)).toBe(true)
      expect(index).toBeGreaterThanOrEqual(0)
      expect(index).toBeLessThan(AVATAR_HUE_COUNT)
    }
  })

  it(`counts the hues in the design tokens`, () => {
    const hues = Object.keys(tokens.avatar).filter(
      (key) => !key.startsWith(`$`)
    )
    expect(hues).toHaveLength(AVATAR_HUE_COUNT)
  })
})
