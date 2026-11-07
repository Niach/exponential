// VAPP-106: `@exponential_app/eui/schema`, the JSON-Schema emitters as their
// own entry: they carry the vendored A2UI basic catalog and write every
// description into the schema, so they attach the descriptions
// (`describeCatalog`) first and stay out of the root entry. Round 9 (§11):
// they emit A2UI 1.0 catalog definitions.

import { describeCatalog } from "../catalog.described"
import { checkCatalogDefinition, coreLiteSchema as bareCoreLiteSchema, coreSchema as bareCoreSchema, extensionSchema as bareExtensionSchema } from "../schema"
import type { ExtensionDef } from "../types"

export { checkCatalogDefinition, describeCatalog }
export type { CheckOptions } from "../schema"

/** The core catalog's 1.0 catalog definition (src/schema.ts `coreSchema`), descriptions included. */
export function coreSchema(iconNames?: readonly string[]): ReturnType<typeof bareCoreSchema> {
  describeCatalog()
  return bareCoreSchema(iconNames)
}

/** The core-lite 1.0 catalog definition (src/schema.ts `coreLiteSchema`), descriptions included. */
export function coreLiteSchema(iconNames?: readonly string[]): ReturnType<typeof bareCoreLiteSchema> {
  describeCatalog()
  return bareCoreLiteSchema(iconNames)
}

/** An extension's 1.0 catalog definition (src/schema.ts `extensionSchema`), descriptions included. */
export function extensionSchema(ext: ExtensionDef, iconNames?: readonly string[]): ReturnType<typeof bareExtensionSchema> {
  describeCatalog()
  return bareExtensionSchema(ext, iconNames)
}
