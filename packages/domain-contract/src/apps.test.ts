import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/apps.json" with { type: "json" }
import { appMediaAllowed, canPublish, editorBrief, editorTab, grantVersion, listPill, previewTemplate, previewTemplates, runAsOptions, shownVersion, slugify, sortApps, versionLabel, versionRow } from "./apps"

describe(`apps (VAPP-110)`, () => {
  test.each(fixture.slugifyCases)(`slugify %j`, (c) => {
    expect(slugify(c.name, (c as { taken?: string[] }).taken ?? [])).toBe(c.slug)
  })
  test.each(fixture.versionLabelCases)(`versionLabel %j`, (c) => {
    expect(versionLabel(c.headVersion)).toBe(c.label)
  })
  test.each(fixture.publishCases.cases)(`publish (V1) $name`, (c) => {
    expect(shownVersion(c.app, c.editing, c.viewing)).toBe(c.shown)
    expect(canPublish(c.app)).toBe(c.canPublish)
    expect(grantVersion(c.app)).toBe(c.grant)
    expect(listPill(c.app)).toBe(c.pill)
    expect(c.rows.map(([n]) => [n, versionRow(c.app, n as number)])).toEqual(c.rows)
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
  test(`a draft's brief asks the agent to name it`, () => {
    expect(editorBrief({ name: `Untitled app`, slug: `untitled-app`, headVersion: 0 })).toContain(`pass name, icon, color and description`)
    expect(editorBrief({ name: `Team dashboard`, slug: `team-dashboard`, headVersion: 1 })).not.toContain(`DRAFT`)
  })
  test.each(fixture.runAsCases)(`runAsOptions %j`, (c) => {
    expect(runAsOptions(c.members, c.me, c.role)).toEqual(c.options as never)
  })
  test.each(fixture.templateCases)(`previewTemplate %j`, (c) => {
    const pkg = { templates: Object.fromEntries(c.templates.map((id) => [id, {}])) }
    expect(previewTemplates(pkg)).toEqual(c.list)
    expect(previewTemplate(pkg, c.picked)).toBe(c.shown)
  })
  test.each(fixture.editorTabCases)(`editorTab %j`, (c) => {
    expect(editorTab(c.requested, c.phone)).toEqual({ tab: c.tab, sheet: c.sheet } as never)
  })
})

describe(`appMediaAllowed (VAPP-157)`, () => {
  test.each(fixture.mediaPolicy.cases)(`$src`, (c) => {
    expect(appMediaAllowed(c.src, fixture.mediaPolicy.origin)).toBe(c.expected)
  })
})
