// VAPP-106: `@exponential_app/ui/schema`, the JSON-Schema emitters as their
// own entry: they carry the vendored A2UI basic catalog and write every
// description into the schema, so they attach the descriptions
// (`describeCatalog`) first and stay out of the root entry.

import { describeCatalog } from "../catalog.described"
import { coreSchema as bareCoreSchema, extensionSchema as bareExtensionSchema } from "../schema"
import type { ExtensionDef } from "../types"

export { describeCatalog }

/** The core catalog as JSON Schema (src/schema.ts `coreSchema`), descriptions included. */
export function coreSchema(iconNames: readonly string[]): ReturnType<typeof bareCoreSchema> {
  describeCatalog()
  return bareCoreSchema(iconNames)
}

/** An extension's schema (src/schema.ts `extensionSchema`), descriptions included. */
export function extensionSchema(ext: ExtensionDef, iconNames: readonly string[]): ReturnType<typeof bareExtensionSchema> {
  describeCatalog()
  return bareExtensionSchema(ext, iconNames)
}
