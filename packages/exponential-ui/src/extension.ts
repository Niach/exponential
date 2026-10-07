// VAPP-85: extension catalogs. An extension = its own catalog id, `extends`
// the core, its components (natives a host paints, macros expanded from its
// own templates) and no name that shadows a core one. The Exponential app's
// extension (IssueRow, RunRow, …) lives with the app (packages/ui) and is the
// first user of this; vapps add theirs the same way.

import { CORE_CATALOG_ID, coreCatalog } from "./catalog"
import type { ExtensionDef } from "./types"

const CATALOG_ID = /^https?:\/\/[^\s]+$/
const NAME = /^[A-Z][A-Za-z0-9]*$/

export function validateExtension(def: ExtensionDef): string[] {
  const errors: string[] = []
  if (!CATALOG_ID.test(def.id)) errors.push(`id must be a URL-shaped catalog id`)
  if (def.extends !== CORE_CATALOG_ID) errors.push(`extends must be ${CORE_CATALOG_ID}`)
  if (!def.name) errors.push(`name is required`)
  const names = Object.keys(def.components)
  if (names.length === 0) errors.push(`an extension defines at least one component`)
  for (const [name, component] of Object.entries(def.components)) {
    if (!NAME.test(name)) errors.push(`${name}: component names are PascalCase`)
    if (name in coreCatalog.components) errors.push(`${name}: shadows a core component`)
    if (!component.description) errors.push(`${name}: description is required`)
    for (const [prop, schema] of Object.entries(component.props)) {
      if (!schema.description) errors.push(`${name}.${prop}: description is required`)
      if (schema.type === `enum` && schema.enum && !(schema.enum in coreCatalog.enums) && !(schema.enum in (def.enums ?? {})))
        errors.push(`${name}.${prop}: enum ${schema.enum} is not defined`)
      if (schema.type === `object` && schema.shape && !(schema.shape in coreCatalog.defs) && !(schema.shape in (def.defs ?? {})))
        errors.push(`${name}.${prop}: shape ${schema.shape} is not defined`)
    }
    if (component.kind === `macro` && !def.macros?.[name]) errors.push(`${name}: a macro needs a template in macros`)
  }
  for (const name of Object.keys(def.macros ?? {})) {
    if (def.components[name]?.kind !== `macro`) errors.push(`macros.${name}: no macro component of that name`)
  }
  for (const name of Object.keys(def.enums ?? {})) {
    if (name in coreCatalog.enums) errors.push(`enums.${name}: shadows a core enum`)
  }
  return errors
}

/** The definition, checked; throws with every problem listed. */
export function defineExtension(def: ExtensionDef): ExtensionDef {
  const errors = validateExtension(def)
  if (errors.length > 0) throw new Error(`extension ${def.id}:\n${errors.join(`\n`)}`)
  return def
}
