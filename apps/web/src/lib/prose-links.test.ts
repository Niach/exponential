import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/prose-links.json"
import { chipBareAppLinks, proseChipKind } from "./prose-links"

// #EXP-1674: replayed ×4 (desktop `domain::prose_links`, iOS `ProseLinksTests`,
// Android `ProseLinksTest`).
describe(`prose-links.json`, () => {
  const synced = fixture.synced as Record<string, string>
  for (const c of fixture.cases) {
    it(c.name, () => {
      expect(chipBareAppLinks(c.text, fixture.origin, fixture.team, (kind, id) => synced[`${kind}:${id}`] ?? null)).toBe(c.out)
    })
  }
  for (const c of fixture.chipKindCases) {
    it(`chip kind: ${c.name}`, () => {
      expect(proseChipKind(c.href)).toBe(c.kind)
    })
  }
  it(`no team, no chips`, () => {
    const text = `${fixture.origin}/t/acme/issues/EXP-42`
    expect(chipBareAppLinks(text, fixture.origin, null, () => `x`)).toBe(text)
  })
})
