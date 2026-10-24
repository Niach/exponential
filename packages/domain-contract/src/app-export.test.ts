import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/app-export.json" with { type: "json" }
import { appExportFileName, appExportText, buildAppExport, parseAppImport } from "./app-export"

describe(`app export (VAPP-121)`, () => {
  test.each(fixture.exportCases)(`buildAppExport: $name`, (c) => {
    const input = c.input as unknown as Parameters<typeof buildAppExport>[0] & { app: { slug: string } }
    expect(appExportFileName(input.app.slug)).toBe(c.expected.fileName)
    const file = buildAppExport(input)
    expect(JSON.parse(appExportText(file))).toEqual(c.expected.file)
  })
  test.each(fixture.importCases)(`parseAppImport: $name`, (c) => {
    expect(parseAppImport(c.text)).toEqual(c.expected as never)
  })
  test(`the capture's import text lists exactly the capture's issues`, () => {
    expect(parseAppImport(fixture.capture.importText).issues).toEqual(fixture.capture.issues)
  })
  test(`an exported file imports back to the same app, package and records`, () => {
    const c = fixture.exportCases[1]!
    const file = buildAppExport(c.input as unknown as Parameters<typeof buildAppExport>[0])
    const parsed = parseAppImport(appExportText(file))
    expect(parsed.issues).toEqual([])
    expect(parsed.file).toEqual({ app: file.app, package: file.package, records: file.records ?? [] })
  })
})
