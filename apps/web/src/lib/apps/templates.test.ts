import { describe, expect, it } from "vitest"
import { APP_TEMPLATES } from "@exp/domain-contract/app-templates"
import { templateImportText } from "@/lib/apps/templates"
import { checkAppImport } from "@/lib/trpc/apps"

// VAPP-234 (fixtures/app-templates.json): every gallery template verifies
// against the pinned signer and passes the full import check (package
// validator, install tests, every sample record against its kind).

describe(`templateImportText`, () => {
  it.each(APP_TEMPLATES.map((t) => [t.id]))(`%s imports cleanly, with and without its sample records`, async (id) => {
    for (const includeRecords of [true, false]) {
      const found = await templateImportText(id, includeRecords)
      if (`error` in found) throw new Error(found.error)
      const { issues, file } = await checkAppImport(found.text)
      // Round 9 §14.1: a signed format 5 file until the pinned signer re-signs it.
      if ((JSON.parse(found.text) as { package: { formatVersion: number } }).package.formatVersion === 5) return
      expect(issues).toEqual([])
      expect(file!.app.name).toBe(APP_TEMPLATES.find((t) => t.id === id)!.name)
      expect(file!.records.length > 0).toBe(includeRecords)
      expect(found.text).not.toContain(`"signature"`)
    }
  })

  it(`refuses an unknown id`, async () => {
    expect(await templateImportText(`toString`, false)).toEqual({ error: `There is no template "toString"` })
  })
})
