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
  // Release train 2026-10-25: Postgres jsonb refuses `\u0000`, so a NUL
  // anywhere in the file is an issue at its pointer, never a 500.
  test(`a NUL in any string or key is an issue at its pointer`, () => {
    const file = {
      format: fixture.format,
      app: { name: `Hours\u0000`, description: `a\u0000b` },
      package: { templates: {}, "a/b": [`ok`, `x\u0000`] },
      records: [{ kind: `entry`, data: { "k\u0000": 1 } }],
    }
    expect(parseAppImport(JSON.stringify(file))).toEqual({
      issues: [
        { path: `/app/name`, message: `must not contain NUL` },
        { path: `/app/description`, message: `must not contain NUL` },
        { path: `/package/a~1b/1`, message: `must not contain NUL` },
        { path: `/records/0/data/k\\u0000`, message: `must not contain NUL` },
      ],
      file: null,
    })
    expect(parseAppImport(JSON.stringify({ name: `Bare\u0000`, templates: {} })).issues).toEqual([{ path: `/name`, message: `must not contain NUL` }])
  })
  test(`an exported file imports back to the same app, package and records`, () => {
    const c = fixture.exportCases[1]!
    const file = buildAppExport(c.input as unknown as Parameters<typeof buildAppExport>[0])
    const parsed = parseAppImport(appExportText(file))
    expect(parsed.issues).toEqual([])
    expect(parsed.file).toEqual({ app: file.app, package: file.package, records: file.records ?? [] })
  })
})
