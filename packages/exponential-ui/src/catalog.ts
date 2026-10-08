// VAPP-85: the catalog as data. `catalog/core.catalog.json` is the source;
// this module loads it, names the catalog ids and answers lookups across the
// core plus any registered extensions.

import coreJson from "../catalog/core.catalog.json" with { type: "json" }
import tokensJson from "../catalog/tokens.json" with { type: "json" }
import basicMapJson from "../catalog/basic-map.json" with { type: "json" }
import macrosJson from "../catalog/macros.json" with { type: "json" }
import type {
  CatalogSource,
  ComponentDef,
  DefSchema,
  ExtensionDef,
  MacroDef,
} from "./types"

export const coreCatalog = coreJson as unknown as CatalogSource

/** `https://ui.exponential.at/catalogs/core/v1` */
export const CORE_CATALOG_ID: string = coreCatalog.id
/** The prompt-sized subset: no overlays, media or Chart. A lite surface is a
 *  valid core surface, so it is a second id over the same definitions. */
export const CORE_LITE_CATALOG_ID: string = coreCatalog.liteId
/** The vendored A2UI basic catalog (vendor/a2ui/v0_9). */
export const A2UI_BASIC_CATALOG_ID: string = basicMapJson.from
/** The A2UI wire `version` the vendored schemas carry. */
export const A2UI_VERSION: string = basicMapJson.version
/** What a client sends as `supportedCatalogIds`. */
export const SUPPORTED_CATALOG_IDS: readonly string[] = [
  CORE_CATALOG_ID,
  CORE_LITE_CATALOG_ID,
  A2UI_BASIC_CATALOG_ID,
]
export const UNKNOWN_COMPONENT: string = coreCatalog.unknownComponent

export const coreMacros = macrosJson.macros as unknown as Record<
  string,
  MacroDef
>

/** Token groups → names, flattened (`type.size`, not a nested object), so a
 *  reference `$<group>.<name>` resolves by one lookup. */
export const TOKEN_GROUPS: Record<string, readonly string[]> = (() => {
  const out: Record<string, readonly string[]> = {}
  for (const [group, value] of Object.entries(tokensJson)) {
    if (group.startsWith(`$`)) continue
    if (Array.isArray(value)) out[group] = value
    else
      for (const [sub, names] of Object.entries(value as Record<string, string[]>))
        out[`${group}.${sub}`] = names
  }
  return out
})()

const TOKEN_REF = /^\$([a-z][a-zA-Z0-9]*(?:\.[a-z][a-zA-Z0-9]*)*)\.([a-zA-Z0-9]+)$/

/** `$spacing.md` → `{ group: "spacing", name: "md" }`, or null when it is not
 *  a well-formed reference. */
export function parseTokenRef(
  value: unknown
): { group: string; name: string } | null {
  if (typeof value !== `string`) return null
  const match = TOKEN_REF.exec(value)
  if (!match) return null
  return { group: match[1], name: match[2] }
}

/** True when the reference names a token group AND a name in it. */
export function isKnownToken(value: unknown): boolean {
  const ref = parseTokenRef(value)
  return ref !== null && (TOKEN_GROUPS[ref.group]?.includes(ref.name) ?? false)
}

export interface CatalogView {
  components: Record<string, ComponentDef>
  enums: Record<string, readonly string[]>
  defs: Record<string, DefSchema>
  macros: Record<string, MacroDef>
}

/** The core merged with the given extensions. Extension names never shadow a
 *  core name (defineExtension refuses that), so a plain spread is exact. */
export function catalogView(extensions: readonly ExtensionDef[] = []): CatalogView {
  const view: CatalogView = {
    components: { ...coreCatalog.components },
    enums: { ...coreCatalog.enums },
    defs: { ...coreCatalog.defs },
    macros: { ...coreMacros },
  }
  for (const ext of extensions) {
    Object.assign(view.components, ext.components)
    Object.assign(view.enums, ext.enums ?? {})
    Object.assign(view.defs, ext.defs ?? {})
    Object.assign(view.macros, ext.macros ?? {})
  }
  return view
}

export function componentDef(
  name: string,
  extensions: readonly ExtensionDef[] = []
): ComponentDef | undefined {
  return catalogView(extensions).components[name]
}

/** Component names in source order, optionally the lite subset, never the
 *  hidden placeholder. */
export function componentNames(options: { lite?: boolean } = {}): string[] {
  return Object.entries(coreCatalog.components)
    .filter(([, def]) => !def.hidden && (!options.lite || def.lite))
    .map(([name]) => name)
}

/** The lite subset's names. */
export function coreLite(): string[] {
  return componentNames({ lite: true })
}
