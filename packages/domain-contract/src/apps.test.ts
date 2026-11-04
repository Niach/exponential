import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/apps.json" with { type: "json" }
import { accountRow, automationSponsor, policyCaption, type PolicyLevel, type PolicyOp, appMediaAllowed, appUploadPath, appUploadTarget, withUploadedUrl, APP_FILES, automationRow, type AutomationRowInput, canPublish, editorBrief, editorTab, grantVersion, listPill, previewTemplate, previewTemplates, runAsOptions, shownVersion, slugify, sortApps, versionLabel, versionMigration, type VersionMigration, versionRow, webhookRow } from "./apps"
import { screenParamsFromQuery, screenPath, screenShell, surfaceState } from "./apps"

describe(`apps (VAPP-110)`, () => {
  test.each(fixture.slugifyCases)(`slugify %j`, (c) => {
    expect(slugify(c.name, (c as { taken?: string[] }).taken ?? [])).toBe(c.slug)
  })
  test.each(fixture.versionLabelCases)(`versionLabel %j`, (c) => {
    expect(versionLabel(c.headVersion)).toBe(c.label)
  })
  test.each(fixture.migrationCases.cases)(`migration (R8) $name`, (c) => {
    expect(versionMigration(c.row as VersionMigration)).toBe(c.expected)
  })
  test.each(fixture.publishCases.cases)(`publish (V1) $name`, (c) => {
    expect(shownVersion(c.app, c.editing, c.viewing)).toBe(c.shown)
    expect(canPublish(c.app)).toBe(c.canPublish)
    expect(grantVersion(c.app)).toBe(c.grant)
    expect(listPill(c.app)).toBe(c.pill)
    expect(c.rows.map(([n]) => [n, versionRow(c.app, n as number)])).toEqual(c.rows)
  })
  test.each(fixture.automationRowCases.cases)(`automationRow (#VAPP-220) $name`, (c) => {
    expect(automationRow(c.row as AutomationRowInput)).toEqual(c.expected)
  })
  test.each(fixture.accountRowCases.cases)(`accountRow (#VAPP-243) $name`, (c) => {
    expect(accountRow(c.row)).toEqual(c.expected as never)
  })
  test.each(fixture.policyRowCases.cases)(`policyCaption (#VAPP-243) $name`, (c) => {
    expect(policyCaption(c.policy as Record<PolicyOp, PolicyLevel>)).toBe(c.expected)
  })
  test.each(fixture.automationSponsorCases.cases)(`automationSponsor (#VAPP-243) $name`, (c) => {
    expect(automationSponsor(c.paused, c.sponsorName)).toBe(c.expected)
  })
  test.each(fixture.webhookRowCases.cases)(`webhookRow (#VAPP-238) $name`, (c) => {
    expect(webhookRow(c.row, c.isOwner)).toEqual(c.expected as never)
  })
  test.each(fixture.surfaceStateCases)(`surfaceState (#VAPP-251) $name`, (c) => {
    expect(surfaceState(c.hasPackage, c.applied, c.hasSurface)).toBe(c.state as never)
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

describe(`appFiles (VAPP-221)`, () => {
  test(`uploadPath`, () => expect(appUploadPath(`a1`)).toBe(`/api/apps/a1/files`))
  test.each(APP_FILES.targetCases)(`appUploadTarget $$why`, (c) => expect(appUploadTarget(c.components as never, c.name)).toBe(c.expected))
  test.each(APP_FILES.urlCases)(`withUploadedUrl $$why`, (c) => expect(withUploadedUrl(c.list, c.fileName, c.url)).toEqual(c.expected))
})
// VAPP-218: the screens contract.
test.each(fixture.screenShellCases)(`screenShell %j`, (c) => expect(screenShell(c.screens, c.phone)).toBe(c.shell as never))
test.each(fixture.screenPathCases)(`screenPath %j`, (c) => expect(screenPath(c.teamSlug, c.appSlug, c.screen, c.params as Record<string, string>)).toBe(c.path))
test.each(fixture.screenQueryCases)(`screenParamsFromQuery %j`, (c) => expect(screenParamsFromQuery(c.query)).toEqual(c.params))
