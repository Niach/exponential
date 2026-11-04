import sharp from "sharp"
import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth/resolve-bearer`, () => ({ resolveSession: vi.fn() }))
vi.mock(`@/lib/team-membership`, () => ({ assertTeamOwner: vi.fn() }))
vi.mock(`@/lib/storage/session-attachment-upload`, () => ({}))

const { normalizeTeamImage } = await import(`@/lib/storage/team-icon-upload`)

const SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10"><rect width="20" height="10" fill="#f00"/></svg>`

describe(`normalizeTeamImage`, () => {
  it(`turns an SVG into a square transparent PNG, nothing cropped`, async () => {
    const png = await normalizeTeamImage(new TextEncoder().encode(SVG))
    const { data, info } = await sharp(png).raw().toBuffer({ resolveWithObject: true })
    expect(info.format).toBe(`raw`)
    expect([info.width, info.height, info.channels]).toEqual([512, 512, 4])
    // Corner = transparent, centre = the drawing.
    expect(data[3]).toBe(0)
    const centre = (256 * 512 + 256) * 4
    expect([data[centre], data[centre + 3]]).toEqual([255, 255])
  })

  it(`refuses bytes that are no image`, async () => {
    await expect(normalizeTeamImage(new TextEncoder().encode(`nope`))).rejects.toThrow(`could not be read`)
  })
})
