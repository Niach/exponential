import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import fixture from "../fixtures/app-templates.json" with { type: "json" }
import exportFixture from "../fixtures/app-export.json" with { type: "json" }
import { parseAppImport } from "./app-export"
import { templateImportFile } from "./app-templates"
import { checkAppTemplates } from "../scripts/sign-app-templates"

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
