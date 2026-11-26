// genUI round 4: fixtures/map-tile.json against the web's imageSourceOf.
import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/map-tile.json" with { type: "json" }
import { imageSourceOf } from "./web/image-actions"

describe(`fixtures/map-tile.json`, () => {
  for (const c of fixture.cases) {
    test(c.name, () => {
      const host = c.resolveUrl === null ? {} : { resolveUrl: (src: string) => (src === c.src ? c.resolveUrl! : src) }
      const verdict = imageSourceOf(host as never, c.src, `Map`)
      expect(verdict.kind === `load` ? `load` : verdict.kind === `text` && verdict.text === `` ? `nothing` : `other`).toBe(c.paints)
    })
  }
})
