import { describe, expect, test } from "bun:test"
import fixture from "../../fixtures/host-package.json" with { type: "json" }
import router from "../../fixtures/host-router.json" with { type: "json" }
import host from "../../catalog/host.json" with { type: "json" }
import {
  ExponentialHost,
  MemoryRecordStore,
  PACKAGE_FILE_SIGNATURE_CONTEXT,
  PACKAGE_FORMAT_VERSION,
  RECORDS_FUNCTIONS,
  RECORDS_QUERY,
  RECORDS_RESERVED_PARAMS,
  RECORDS_SCHEME,
  TOOL_ID_PATTERN,
  base64UrlDecode,
  canonicalJson,
  packageFileSignedBytes,
  parseRecordsSource,
  parseSource,
  recordsHost,
  recordsValue,
  signPackageFile,
  supportedCatalogIds,
  toolCall,
  toolConsent,
  toolConsentKey,
  validatePackage,
  verifyPackageFile,
} from "."
import type { AppPackage, AvailableExtension, PackageTool, RecordRow } from "."

// VAPP-161: the package format (docs/package-format.md) against
// fixtures/host-package.json; validation itself replays host-router.json.

const parsed = (source: string) => {
  const p = parseSource(source)
  return p ? parseRecordsSource(p) : null
}

describe(`package format (VAPP-161)`, () => {
  test(`the constants are the catalog's`, () => {
    expect(PACKAGE_FORMAT_VERSION).toBe(host.package.formatVersion)
    expect(RECORDS_SCHEME).toBe(host.package.records.scheme)
    expect([...RECORDS_FUNCTIONS]).toEqual(host.package.records.functions)
    expect(RECORDS_QUERY).toEqual({ ...host.package.records.query, reserved: undefined } as never)
    expect([...RECORDS_RESERVED_PARAMS]).toEqual(host.package.records.query.reserved)
    expect(TOOL_ID_PATTERN.source).toBe(host.package.tools.idPattern)
    expect(PACKAGE_FILE_SIGNATURE_CONTEXT).toBe(host.package.signature.context)
  })

  test(`every version-1 package of the router fixture stays valid`, () => {
    for (const id of [`acme.devices`, `acme.scoped`, `acme.hours`, `acme.v1`]) expect(validatePackage((router.packages as Record<string, unknown>)[id])).toEqual([])
  })

  test.each(fixture.recordsQuery)(`parseRecordsSource $source`, (c) => {
    expect(parsed(c.source)).toEqual(c.expected as never)
  })
  test.each(fixture.recordsValue)(`recordsValue: $name`, (c) => {
    expect(recordsValue(c.rows as RecordRow[], parsed(c.source)!, { me: c.me, names: c.names as Record<string, string> })).toEqual(c.expected as never)
  })
  test.each(fixture.consent)(`toolConsent $packageId`, (c) => {
    const tools = ((c.package as { tools?: Record<string, PackageTool> }).tools ?? {}) as Record<string, PackageTool>
    expect(toolConsent(c.package as { tools?: Record<string, PackageTool> })).toEqual(c.expected as never)
    for (const [id, key] of Object.entries(c.keys)) expect(toolConsentKey(c.packageId, id, tools[id]!)).toBe(key as string)
  })
  test(`a changed tool is a new consent key`, () => {
    const tool: PackageTool = { kind: `mcp`, title: `Weather`, url: `https://a.example/mcp` }
    expect(toolConsentKey(`p`, `w`, tool)).not.toBe(toolConsentKey(`p`, `w`, { ...tool, url: `https://b.example/mcp` }))
    expect(toolConsentKey(`p`, `w`, tool)).toBe(toolConsentKey(`p`, `w`, { url: tool.url, title: tool.title, kind: `mcp` }))
  })
  test.each(fixture.toolCalls)(`toolCall $name`, (c) => {
    expect(toolCall(c.name)).toEqual(c.expected as never)
  })
  test.each(fixture.extensions)(`extensions: $name`, (c) => {
    const pkg = { id: `x`, name: `X`, version: `1.0.0`, catalogId: supportedCatalogIds()[0]!, templates: { t: { components: [{ id: `root`, component: `Text`, text: `Hi` }] } }, extensions: c.extensions }
    expect(validatePackage(pkg, supportedCatalogIds(c.extensionIds), (c as { available?: AvailableExtension[] }).available)).toEqual(c.expected as never)
  })
  test.each(fixture.canonical)(`canonicalJson %#`, (c) => {
    expect(canonicalJson(JSON.parse(c.text))).toBe(c.expected)
  })
  test(`canonicalJson refuses what JSON cannot spell`, () => {
    expect(() => canonicalJson({ n: Number.NaN })).toThrow()
    expect(() => canonicalJson({ n: Infinity })).toThrow()
    expect(canonicalJson({ a: undefined, b: [undefined] })).toBe(`{"b":[null]}`)
  })

  test.each(fixture.signatures)(`verifyPackageFile: $name`, async (c) => {
    expect(new TextDecoder().decode(packageFileSignedBytes(c.signed as Record<string, unknown>))).toBe(c.signedText)
    expect(await verifyPackageFile(c.signed)).toEqual(c.expected as never)
  })
  test(`signPackageFile = the generator's node:crypto signature (Ed25519 is deterministic)`, async () => {
    const seed = Uint8Array.from(Buffer.from(fixture.key.seed, `hex`))
    const signed = fixture.signatures.find((c) => c.name === `signed with the test key`)!
    const ours = await signPackageFile(signed.file as Record<string, unknown>, { name: fixture.key.name, seed })
    expect(ours).toEqual(signed.signed as never)
    expect(Buffer.from(base64UrlDecode(ours.author.key.slice(`ed25519:`.length))!).toString(`hex`)).toBe(fixture.key.publicKey)
    expect((await verifyPackageFile({ ...ours, records: [] })).status).toBe(`invalid`)
  })
})

describe(`records on a host (VAPP-161)`, () => {
  const pkg = (router.packages as Record<string, unknown>)[`acme.v2`] as AppPackage

  test(`recordsHost: the binding follows the store, writes are validated first`, async () => {
    let t = 0
    const store = new MemoryRecordStore({ me: `u1`, now: () => `2026-10-0${++t}T00:00:00Z` })
    const records = recordsHost(pkg, store, { me: `u1`, names: { u1: `Ada` } })
    const h = new ExponentialHost({ packages: [pkg], sources: records.sources, functions: records.functions })
    const seen: unknown[] = []
    const cancel = records.sources[RECORDS_SCHEME]!(parseSource(`records:entry?sort=date&order=asc`)!, (v) => seen.push(v))
    expect(seen.at(-1)).toEqual({ rows: [], count: 0 })
    const { id } = (await records.functions[`records.upsert`]!({ kind: `entry`, data: { date: `2026-10-05`, hours: 3 } })) as { id: string }
    await records.functions[`records.upsert`]!({ kind: `entry`, data: { date: `2026-10-04` } })
    expect((seen.at(-1) as { rows: { date: string; author: string }[] }).rows.map((r) => r.date)).toEqual([`2026-10-04`, `2026-10-05`])
    await records.functions[`records.upsert`]!({ id, data: { hours: 4 } })
    expect((seen.at(-1) as { rows: { id: string; hours?: number; author: string }[] }).rows.find((r) => r.id === id)).toMatchObject({ hours: 4, date: `2026-10-05`, author: `Ada` })
    await expect(records.functions[`records.upsert`]!({ kind: `entry`, data: { hours: 1 } })).rejects.toThrow(`/date required`)
    await expect(records.functions[`records.upsert`]!({ id, data: { date: `soon` } })).rejects.toThrow(`/date`)
    await expect(records.functions[`records.upsert`]!({ kind: `nope`, data: {} })).rejects.toThrow(`no kind nope`)
    await records.functions[`records.delete`]!({ id })
    expect((seen.at(-1) as { count: number }).count).toBe(1)
    const undeclared: unknown[] = []
    records.sources[RECORDS_SCHEME]!(parseSource(`records:other`)!, (v) => undeclared.push(v))
    expect(undeclared).toEqual([null])
    ;(cancel as () => void)()
    expect(h.issues).toEqual([])
  })

  test(`a host refuses a package that needs an extension it lacks, at its version`, () => {
    const charts = (router.packages as Record<string, unknown>)[`acme.charts`] as AppPackage
    const ext = { id: `https://acme.example/catalogs/charts/v1`, name: `Charts`, extends: supportedCatalogIds()[0]!, components: {} }
    expect(new ExponentialHost({ extensions: [{ ...ext, version: `1.5.0` }] }).installPackage(charts)).toEqual([
      { path: `/extensions/0/components/0`, message: `extension ${ext.id} has no component Gantt` },
    ])
    expect(new ExponentialHost({ extensions: [{ ...ext, version: `1.3.0`, components: { Gantt: { description: `` } as never } }] }).installPackage(charts)).toEqual([
      { path: `/extensions/0/version`, message: `extension ${ext.id} 1.3.0 does not satisfy ^1.4.0` },
    ])
    expect(new ExponentialHost().installPackage(charts)[0]!.message).toContain(`is not available on this host`)
  })
})
