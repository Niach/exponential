import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import fixture from "../fixtures/app-templates.json" with { type: "json" }
import exportFixture from "../fixtures/app-export.json" with { type: "json" }
import { parseAppImport } from "./app-export"
import { templateImportFile } from "./app-templates"
import { SIGNED_SAMPLES, checkAppTemplates, checkSignedSamples } from "../scripts/sign-app-templates"
import { signPackageFile, verifyPackageFile } from "../../exponential-ui/src/host/signature"

const root = join(import.meta.dir, `..`, `..`, `..`)
const read = (id: string) => readFileSync(join(root, fixture.dir, `${id}${fixture.fileSuffix}`), `utf8`)

describe(`app templates (VAPP-234)`, () => {
  test.each(fixture.importCases)(`templateImportFile: $name`, (c) => {
    expect(templateImportFile(c.input.file, c.input.includeRecords)).toEqual(c.expected)
  })
  test(`every template is signed by the pinned signer`, async () => {
    expect(await checkAppTemplates()).toEqual([])
  })
  test.each(fixture.templates)(`$id: the list entry is the file's identity, and it imports cleanly`, (t) => {
    const file = JSON.parse(read(t.id))
    expect(file.format).toBe(exportFixture.format)
    const { description: _d, ...app } = file.app
    const { id: _id, description: _ld, ...entry } = t
    expect(app).toEqual(entry)
    for (const includeRecords of [true, false]) {
      const parsed = parseAppImport(JSON.stringify(templateImportFile(file, includeRecords)))
      expect(parsed.issues).toEqual([])
      expect(parsed.file?.records.length ?? 0).toBe(includeRecords ? file.records.length : 0)
    }
    expect(file.records.length).toBeGreaterThan(0)
    for (const record of file.records) expect(record.author).toBeUndefined()
  })
})

// VAPP-271: the Photobook is signed by the same pinned signer, but it is no gallery template.
describe(`signed samples (VAPP-271)`, () => {
  test(`none is a template`, () => {
    for (const id of SIGNED_SAMPLES) expect(fixture.templates.map((t) => t.id)).not.toContain(id)
    const gallery = readFileSync(join(root, `apps/web/src/lib/apps/templates.ts`), `utf8`)
    for (const id of SIGNED_SAMPLES) expect(gallery).not.toContain(`${id}${fixture.fileSuffix}`)
  })
  test(`each is signed by the pinned signer, or still unsigned`, async () => {
    expect((await checkSignedSamples()).problems).toEqual([])
  })
  test(`each is a package file the signer can sign`, async () => {
    for (const id of SIGNED_SAMPLES) {
      const { author: _a, signature: _s, ...file } = JSON.parse(read(id)) as Record<string, unknown>
      expect(Object.keys(file)).toEqual([`package`])
      // RFC 8032 §7.1 TEST 1's seed, never a real key.
      const seed = Uint8Array.from(Buffer.from(`9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60`, `hex`))
      const signed = await signPackageFile(file, { name: fixture.signer.name, seed })
      expect((await verifyPackageFile(signed)).status).toBe(`valid`)
    }
  })
})
