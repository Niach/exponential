// VAPP-234: the template gallery's pure reference (fixtures/app-templates.json
// is the spec). The server verifies a template file's pinned signature, then
// imports `templateImportFile(file, includeRecords)` exactly like an
// `.app.json` import. The clients mirror only the list and the copy.

import fixture from "../fixtures/app-templates.json" with { type: "json" }

export interface AppTemplate {
  id: string
  name: string
  icon: string
  color: string
  description: string
}

export const APP_TEMPLATES: readonly AppTemplate[] = fixture.templates
export const APP_TEMPLATES_COPY = fixture.copy
export const APP_TEMPLATES_TEST_IDS = fixture.testIds
export const APP_TEMPLATES_DIR: string = fixture.dir
export const APP_TEMPLATES_SIGNER: { name: string; key: string } = fixture.signer
export const APP_TEMPLATE_IDS = fixture.templates.map((t) => t.id) as [string, ...string[]]

/** The file `apps.importTemplate` imports: the template without its
 *  `author` + `signature` (verified before), and without its sample
 *  `records` unless the person asked for them. */
export function templateImportFile(file: Record<string, unknown>, includeRecords: boolean): Record<string, unknown> {
  const { author: _author, signature: _signature, records, ...rest } = file
  return includeRecords && records !== undefined ? { ...rest, records } : rest
}
