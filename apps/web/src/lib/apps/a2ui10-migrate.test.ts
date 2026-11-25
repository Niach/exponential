// @vitest-environment node
// VAPP-289 (round 9 §14.1): the migration's pure half: what each stored value
// becomes, the rows the tables would refuse, and every package file the
// platform ships. The database half is a2ui10-migrate.real-db.test.ts.
import { readFileSync, readdirSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { describe, expect, it, vi } from "vitest"
import { CORE_CATALOG_ID, cardProblems, cardV10, reduceNested, signPackageFile, validatePackage, verifyPackageFile, type NestedNode, type PackageShare, type RecordKind } from "@exponential_app/eui"
import { declaredActions } from "@exp/domain-contract/app-actions"
import { APP_PACKAGE_MAX_BYTES } from "@exp/domain-contract/apps"
import fixture from "./a2ui10-migrate.fixture.json"

vi.mock(`@/db/connection`, () => ({ db: {} }))

const { upgradeGrants, upgradePackageFile, upgradeStagedRows, upgradeStoredCard, upgradeStoredPackage } = await import(`@/lib/apps/a2ui10-migrate`)
const { APP_CATALOG_IDS, appExtension, appPackageIssues, linkableScreens, migratedPackageIssues } = await import(`@/lib/apps/package`)
const { guestPackage } = await import(`@/lib/apps/share-links`)

const ROOT = join(dirname(fileURLToPath(import.meta.url)), `..`, `..`, `..`, `..`, `..`)
const json = (path: string) => JSON.parse(readFileSync(join(ROOT, path), `utf8`)) as Record<string, unknown>
const inDir = (dir: string, suffix: string) => readdirSync(join(ROOT, dir)).filter((f) => f.endsWith(suffix)).map((f) => join(dir, f))

/** Every package the platform ships as a file. */
const SHIPPED: readonly string[] = [
  ...inDir(`samples/apps`, `.app.json`),
  ...inDir(`packages/exponential-ui/skills/exponential-apps/examples`, `.app.json`),
  ...inDir(`packages/ui/exponential-ui/templates`, `.json`),
  `apps/runner/web/public/app/package.app.json`,
  `apps/runner/web/samples/album.app.json`,
  `apps/runner/ios/sample/app/package.app.json`,
  `apps/runner/android/app/src/main/assets/app/package.app.json`,
  `apps/desktop/crates/ui/src/app_screen.test-package.json`,
]

const bodyOf = (file: Record<string, unknown>) => (file.package ?? file) as Record<string, unknown>
const rewritten = (verdict: { kind: string; value?: unknown; reason?: string }) => {
  expect(verdict.reason).toBeUndefined()
  expect(verdict.kind).toBe(`rewrite`)
  return verdict.value as Record<string, unknown>
}

/** A shipped file as a client reads it: at format 6, or (the signed samples,
 *  until the pinned signer re-signs them) its upgrade. */
const atFormat6 = async (body: Record<string, unknown>) => {
  const verdict = await upgradeStoredPackage(body)
  return verdict.kind === `current` ? body : rewritten(verdict)
}

describe(`upgradeStoredPackage`, () => {
  it(`takes a real-shaped package to a format 6 every client loads`, async () => {
    const pkg = rewritten(await upgradeStoredPackage(fixture.packages.fieldVisits))
    expect(pkg.formatVersion).toBe(6)
    expect(validatePackage(pkg, APP_CATALOG_IDS)).toEqual([])
    const text = JSON.stringify(pkg)
    for (const old of [`"records.upsert"`, `"actions.logVisit"`, `"harness.toast"`, `"app.navigate"`, `"exponential.issues.create"`, `tools.crm-hub`, `"tool-bar"`, `"detail.body"`, `"path":"/draft/title"`]) expect(text).not.toContain(old)
    expect(text).toContain(`"tools_crm_hub__contacts_search"`)
    expect(text).toContain(`"run":{"action":"exponential_issues_create"`)
    expect(Object.keys(pkg.tools as object)).toEqual([`crm_hub`])
  })

  it(`skips format 6, reports what it cannot read`, async () => {
    expect(await upgradeStoredPackage(fixture.packages.alreadyV6)).toEqual({ kind: `current` })
    expect(await upgradeStoredPackage(null)).toEqual({ kind: `empty` })
    expect(await upgradeStoredPackage(fixture.packages.formatFour)).toEqual({ kind: `failed`, reason: `format version 4 has no upgrade` })
    expect(await upgradeStoredPackage([])).toEqual({ kind: `failed`, reason: `a package is an object` })
    const broken = await upgradeStoredPackage(fixture.packages.brokenTemplate)
    expect(broken).toMatchObject({ kind: `failed`, reason: expect.stringContaining(`no component has the id root`) })
  })

  it(`a package the upgrade grows past the cap fails, never truncates`, async () => {
    const base = structuredClone(fixture.packages.fieldVisits) as Record<string, unknown>
    const bytes = (value: unknown) => new TextEncoder().encode(JSON.stringify(value)).byteLength
    const before = bytes(base)
    const growth = bytes(rewritten(await upgradeStoredPackage(base))) - before
    expect(growth).toBeGreaterThan(0)
    // Fills the original to the cap exactly: it fits, its upgrade cannot.
    base.description = `${base.description}${`x`.repeat(APP_PACKAGE_MAX_BYTES - before)}`
    expect(bytes(base)).toBe(APP_PACKAGE_MAX_BYTES)
    expect(await upgradeStoredPackage(base)).toMatchObject({ kind: `failed`, reason: expect.stringContaining(`the cap is ${APP_PACKAGE_MAX_BYTES}`) })
  })
})

describe(`upgradeStoredCard`, () => {
  const load = (card: { surface: unknown }) => {
    const entry = cardV10(card.surface)
    if (`error` in entry) throw new Error(entry.error)
    expect(cardProblems(entry.card.root)).toEqual([])
    expect(reduceNested(entry.card.root as NestedNode, { catalogId: CORE_CATALOG_ID }).issues).toEqual([])
  }

  for (const name of [`surface`, `surfaceTable`, `plan`, `question`] as const) {
    it(`a ${name} card gets version 1.0 and loads with the bridge off`, () => {
      const card = rewritten(upgradeStoredCard(fixture.cards[name])) as { surface: { version: string; root: { id: string } } }
      expect(card.surface.version).toBe(`1.0`)
      load(card)
      const { surface: _before, ...rest } = fixture.cards[name] as Record<string, unknown>
      expect(card).toMatchObject(JSON.parse(JSON.stringify(rest)))
    })
  }

  it(`re-keys ids and references, never data keys`, () => {
    const card = rewritten(upgradeStoredCard(fixture.cards.surface)) as { surface: unknown }
    const text = JSON.stringify(card.surface)
    expect(text).toContain(`"launch_row"`)
    expect(text).toContain(`"for":"pick_field"`)
    expect(text).toContain(`{"@path":"/title"}`)
    const question = rewritten(upgradeStoredCard(fixture.cards.question)) as { surfaceData: unknown }
    expect(question.surfaceData).toEqual(fixture.cards.question.surfaceData)
  })

  it(`skips sealed cards, 1.0 cards and cards with no surface`, () => {
    expect(upgradeStoredCard(fixture.cards.sealed)).toEqual({ kind: `sealed` })
    expect(upgradeStoredCard(fixture.cards.sealedAsk)).toEqual({ kind: `sealed` })
    expect(upgradeStoredCard(fixture.cards.already10)).toEqual({ kind: `current` })
    expect(upgradeStoredCard(fixture.cards.noSurface)).toEqual({ kind: `empty` })
  })

  it(`reports what does not load`, () => {
    expect(upgradeStoredCard(fixture.cards.brokenComponent)).toMatchObject({ kind: `failed`, reason: expect.stringContaining(`NoSuchComponent`) })
    expect(upgradeStoredCard(fixture.cards.brokenVersion)).toEqual({ kind: `failed`, reason: `unsupported surface version "0.8"` })
  })
})

describe(`upgradeGrants`, () => {
  it(`flattens in stored order, de-duplicated`, () => {
    expect(upgradeGrants(fixture.grants.dotted)).toEqual({ kind: `rewrite`, value: [`exponential_issues_update`, `exponential_mcp_servers_list`, `exponential_issues_create`] })
    expect(upgradeGrants(fixture.grants.mixed)).toEqual({ kind: `rewrite`, value: [`exponential_issues_update`, `exponential_issues_create`] })
    expect(upgradeGrants(fixture.grants.flat)).toEqual({ kind: `current` })
    expect(upgradeGrants([])).toEqual({ kind: `current` })
    expect(upgradeGrants(`x`)).toMatchObject({ kind: `failed` })
  })
})

describe(`every package the platform ships`, () => {
  for (const path of SHIPPED) {
    it(`${path} loads at format 6`, async () => {
      const pkg = await atFormat6(bodyOf(json(path)))
      expect(pkg.formatVersion).toBe(6)
      expect(migratedPackageIssues(pkg)).toEqual([])
      expect(validatePackage(pkg, APP_CATALOG_IDS)).toEqual([])
      expect(JSON.stringify(pkg)).not.toMatch(/"(call|action)":\s*"(records|actions|harness|app|exponential)\./)
    })
  }

  for (const path of inDir(`samples/apps`, `.app.json`)) {
    it(`${path} passes the flag-day upsert`, async () => {
      expect(appPackageIssues(await atFormat6(bodyOf(json(path))))).toEqual([])
    })
  }

  it(`names the app extension it validated against`, () => {
    expect(APP_CATALOG_IDS).toContain(appExtension.id)
  })
})

describe(`upgradeStagedRows`, () => {
  it(`upgrades what a team import would write, column by column`, async () => {
    const rows = [
      { tableName: `apps`, localId: `a`, row: { id: `a`, package: fixture.packages.fieldVisits, draft_package: null, granted_tools: fixture.grants.dotted } },
      { tableName: `app_versions`, localId: `v`, row: { id: `v`, package: fixture.packages.fieldVisits } },
      { tableName: `messages`, localId: `m`, row: { id: `m`, card: fixture.cards.surface } },
      { tableName: `issues`, localId: `i`, row: { id: `i`, package: `not a package` } },
    ]
    const [app, version, message, issue] = await upgradeStagedRows(rows)
    expect((app!.row.package as { formatVersion: number }).formatVersion).toBe(6)
    expect(app!.row.granted_tools).toEqual([`exponential_issues_update`, `exponential_mcp_servers_list`, `exponential_issues_create`])
    expect((version!.row.package as { formatVersion: number }).formatVersion).toBe(6)
    expect((message!.row.card as unknown as { surface: { version: string } }).surface.version).toBe(`1.0`)
    expect(issue).toEqual(rows[3])
  })
})

describe(`a stored format 6 row`, () => {
  const row = async () => rewritten(await upgradeStoredPackage(fixture.packages.fieldVisits))

  it(`the server's readers take it as it is`, async () => {
    const pkg = await row()
    expect(declaredActions(pkg.functions as string[])).toContain(`exponential_issues_create`)
    expect(linkableScreens(`visits`, pkg).map((s) => s.screen)).toEqual([`visits`])
    expect(appPackageIssues(pkg).map((issue) => issue.path)).toEqual([`/tools`])
  })

  it(`a share link serves its guest template from it`, async () => {
    const pkg = await row()
    const kind = (pkg.records as Record<string, RecordKind>).visit!
    const share: PackageShare = { name: `board`, mode: `view`, kind: `visit`, fields: [`title`], template: `visits` }
    const stored = { ...pkg, templates: { visits: { components: [{ id: `root`, component: `Page`, title: `Board`, children: [`rows_table`] }, { id: `rows_table`, component: `RecordTable`, kind: `visit`, editable: false }] } } }
    const guest = guestPackage(stored, share, kind, [`title`], `Visits`)
    expect(guest && validatePackage(guest)).toEqual([])
  })
})

describe(`upgradePackageFile (§18.1)`, () => {
  const seed = Uint8Array.from(Buffer.from(`9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60`, `hex`))
  const file = () => ({ format: 1, app: { name: `Field visits` }, package: fixture.packages.fieldVisits, records: [] })

  it(`re-signs a file signed with the key we hold`, async () => {
    const signed = await signPackageFile(file(), { name: `Exponential`, seed })
    const out = await upgradePackageFile(signed, { name: `Exponential`, seed })
    expect(out).toMatchObject({ status: `valid`, resigned: true })
    expect((out.file!.package as { formatVersion: number }).formatVersion).toBe(6)
    expect(await verifyPackageFile(out.file)).toMatchObject({ status: `valid`, author: signed.author })
  })

  it(`a file signed by someone else keeps its verified status, unsigned`, async () => {
    const signed = await signPackageFile(file(), { name: `Stranger`, seed })
    const other = Uint8Array.from({ length: 32 }, (_, i) => i)
    const out = await upgradePackageFile(signed, { name: `Exponential`, seed: other })
    expect(out).toMatchObject({ status: `valid`, resigned: false })
    expect(out.file!.signature).toBeUndefined()
    expect(out.file!.author).toEqual(signed.author)
  })

  it(`the repo's signed samples verify; we hold no seed here`, async () => {
    for (const path of inDir(`samples/apps`, `.app.json`)) {
      const out = await upgradePackageFile(json(path))
      expect(out.status).toBe(`valid`)
      expect((out.file!.package as { formatVersion: number }).formatVersion).toBe(6)
    }
  })

  it(`a tampered file gives no output, a bare unsigned one upgrades`, async () => {
    const signed = await signPackageFile(file(), { name: `Exponential`, seed })
    expect(await upgradePackageFile({ ...signed, records: [{ kind: `x` }] })).toEqual({ file: null, status: `invalid`, resigned: false })
    const bare = await upgradePackageFile(fixture.packages.fieldVisits)
    expect(bare).toMatchObject({ status: `unsigned`, resigned: false })
    expect(bare.file!.formatVersion).toBe(6)
  })
})
