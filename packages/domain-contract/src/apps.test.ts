import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/apps.json" with { type: "json" }
import { appMediaAllowed, chatCard, editorBrief, editorLayout, slugify, sortApps, versionLabel } from "./apps"

describe(`apps (VAPP-110)`, () => {
  test.each(fixture.slugifyCases)(`slugify %j`, (c) => {
    expect(slugify(c.name, (c as { taken?: string[] }).taken ?? [])).toBe(c.slug)
  })
  test.each(fixture.versionLabelCases)(`versionLabel %j`, (c) => {
    expect(versionLabel(c.headVersion)).toBe(c.label)
  })
  test.each(fixture.sortAppsCases)(`sortApps %j`, (c) => {
    expect(sortApps(c.apps).map((a) => a.slug)).toEqual(c.order)
  })
  test(`the editor brief names the slug and the three MCP calls`, () => {
    const brief = editorBrief({ name: `Team dashboard`, slug: `team-dashboard`, headVersion: 3 })
    expect(brief).toContain(`slug \`team-dashboard\``)
    expect(brief).toContain(`v3`)
    for (const op of [`"get"`, `"catalog"`, `"upsert"`, `"records_upsert"`, `"records_list"`]) expect(brief).toContain(op)
  })
  test.each(fixture.chatCardCases)(`chatCard %j`, (c) => {
    const row = chatCard(c.messages, c.busy)
    expect(row.title).toBe(c.title)
    expect(row.caption).toBe(c.caption)
    expect(row).toMatchObject({ id: fixture.dockCard.id, kind: `chat`, tint: `muted`, expandable: true, action: null })
  })
  test(`the layout falls back to the default`, () => {
    expect(editorLayout(null)).toBe(fixture.layout.default)
    expect(editorLayout(`docked`)).toBe(`docked`)
    expect(editorLayout(`sideways`)).toBe(fixture.layout.default)
  })
})

describe(`appMediaAllowed (VAPP-157)`, () => {
  test.each(fixture.mediaPolicy.cases)(`$src`, (c) => {
    expect(appMediaAllowed(c.src, fixture.mediaPolicy.origin)).toBe(c.expected)
  })
})
