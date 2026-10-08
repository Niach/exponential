/* The site's data: the generated catalog docs, theme docs and specimens of
   @exponential-at/ui (docs/*.generated.json, fixtures/specimens.json), plus
   the shot index (scripts/shot-index.ts). One page per component; its slug
   is its specimen id minus the prefix. */
import componentsDoc from "@exponential-at/ui/docs/components.generated.json"
import themesDoc from "@exponential-at/ui/docs/themes.generated.json"
import macrosDoc from "@exponential-at/ui/catalog/macros.json"
import specimensDoc from "@exponential-at/ui/fixtures/specimens.json"

export type ComponentDoc = (typeof componentsDoc.components)[number]
export type ThemesDoc = typeof themesDoc
export type Specimen = (typeof specimensDoc.specimens)[number]

export const COMPONENT_DOCS: readonly ComponentDoc[] = componentsDoc.components
export const COMPONENT_GROUPS: readonly string[] = componentsDoc.groups
export const CATALOG_ID: string = componentsDoc.catalogId
export const LITE_CATALOG_ID: string = componentsDoc.liteCatalogId
export const THEMES_DOC: ThemesDoc = themesDoc
export const SPECIMENS: readonly Specimen[] = specimensDoc.specimens

const PREFIX = `exponential-ui-`

export const componentSlug = (doc: ComponentDoc) => doc.specimenId.slice(PREFIX.length)
export const componentPath = (doc: ComponentDoc) => `/components/${componentSlug(doc)}/`
export const componentBySlug = (slug: string) => COMPONENT_DOCS.find((d) => componentSlug(d) === slug)
export const componentByName = (name: string) => COMPONENT_DOCS.find((d) => d.name === name)
export const specimenById = (id: string) => SPECIMENS.find((s) => s.id === id)

/** The four platforms every specimen is photographed on, in page order. */
export const SHOT_PLATFORMS = [
  { id: `web`, label: `Web`, renderer: `React` },
  { id: `ios`, label: `iOS`, renderer: `SwiftUI` },
  { id: `android`, label: `Android`, renderer: `Compose` },
  { id: `desktop`, label: `Desktop`, renderer: `gpui` },
] as const

/** The stored shot of a view on a platform (the build copies shots/ into dist). */
export const shotUrl = (viewId: string, platform: string) => `/shots/${viewId}/${platform}.webp`

/** The groups in page order (the index, the sidebar). */
export const SITE_GROUP_ORDER = [`layout`, `text`, `media`, `navigation`, `overlay`, `feedback`, `data`, `action`, `input`, `list`] as const

export function groupedDocs(): { group: string; docs: ComponentDoc[] }[] {
  const known = [...SITE_GROUP_ORDER, ...COMPONENT_GROUPS.filter((g) => !(SITE_GROUP_ORDER as readonly string[]).includes(g))]
  const groups = known.map((group) => ({ group, docs: COMPONENT_DOCS.filter((d) => d.group === group) })).filter((g) => g.docs.length > 0)
  const other = COMPONENT_DOCS.filter((d) => !known.includes(d.group))
  if (other.length) groups.push({ group: `other`, docs: other })
  return groups
}

/** Every component in page order (grouped), for prev/next and the sidebar. */
export const ORDERED_DOCS: readonly ComponentDoc[] = groupedDocs().flatMap((g) => g.docs)

export const SUMMARY_MAX = 160

/** The one sentence the site prints for a component: the model-facing
 *  description, cut at a clause boundary when it runs past 160 chars. */
export function componentSummary(doc: Pick<ComponentDoc, `description`>): string {
  const text = doc.description.trim()
  if (text.length <= SUMMARY_MAX) return text
  const head = text.slice(0, SUMMARY_MAX - 1)
  for (const sep of [`; `, `, `, ` `]) {
    const at = head.lastIndexOf(sep)
    if (at > 40) return `${head.slice(0, at).replace(/[,;:\s]+$/, ``)}.`
  }
  return `${head}.`
}

type MacroTree = unknown
const MACROS = (macrosDoc as { macros: Record<string, MacroTree> }).macros

/** The natives (and core components) a macro expands into, sorted. */
export function macroParts(name: string): string[] {
  const tree = MACROS[name]
  if (!tree) return []
  const out = new Set<string>()
  const walk = (n: unknown) => {
    if (Array.isArray(n)) n.forEach(walk)
    else if (n && typeof n === `object`) {
      const c = (n as { component?: unknown }).component
      if (typeof c === `string` && c !== name) out.add(c)
      Object.values(n).forEach(walk)
    }
  }
  walk(tree)
  return [...out].filter((c) => componentByName(c)).sort()
}

/** Components a page links as "Related": the ones its sentence names, what
 *  it is built from, what is built from it, then its group. */
export function relatedComponents(doc: ComponentDoc, max = 8): ComponentDoc[] {
  const names = new Set<string>()
  for (const d of COMPONENT_DOCS) {
    if (d.name === doc.name) continue
    if (new RegExp(`(^|[^A-Za-z])${d.name}([^A-Za-z]|$)`).test(doc.description)) names.add(d.name)
  }
  for (const n of macroParts(doc.name)) if (n !== `Box`) names.add(n)
  for (const d of COMPONENT_DOCS) if (d.name !== doc.name && macroParts(d.name).includes(doc.name) && doc.name !== `Box` && doc.name !== `Text`) names.add(d.name)
  for (const d of COMPONENT_DOCS) if (d.group === doc.group && d.name !== doc.name) names.add(d.name)
  return [...names].slice(0, max).map((n) => componentByName(n)!)
}

/** One keyboard line of a component's a11y entry: the keys, then what they do
 *  (a line without "keys: action" is a note). */
export interface KeyRow {
  keys: string[]
  action: string
}

export function keyboardRows(doc: ComponentDoc): KeyRow[] {
  return (doc.accessibility?.keys ?? []).map((line) => {
    const at = line.indexOf(`: `)
    if (at < 0 || at > 48) return { keys: [], action: line }
    return {
      keys: line
        .slice(0, at)
        .split(/\s*(?:,|\/| or )\s*/)
        .map((k) => k.trim())
        .filter(Boolean),
      action: line.slice(at + 2),
    }
  })
}

/** The variants gallery: every option of every short enum prop and every
 *  state-like boolean, as props over the component's starting props. */
export interface Variant {
  label: string
  prop: string
  props: Record<string, unknown>
}

/* Props that change behaviour, not looks, stay out of the gallery. */
const SKIP_PROPS = new Set([`open`, `pressable`, `multiple`, `autoplay`, `loop`, `dragToDismiss`, `submit`, `searchable`, `autosize`, `stickyHeaders`, `stickyHeader`, `dismissible`, `live`, `loading`, `validateOn`, `openOn`, `collapseBelow`, `gap`, `justify`, `align`, `selectable`, `side`, `external`])
const SKIP_QUALIFIED = new Set([`Input.type`, `Image.loading`])

export function variantsOf(doc: ComponentDoc, base: Record<string, unknown>, enumOptions: (type: string) => string[] | null, max = 24): Variant[] {
  if (doc.group === `overlay`) return []
  const out: Variant[] = []
  for (const p of doc.props) {
    const prop = p as { name: string; type: string; default?: unknown }
    if (SKIP_PROPS.has(prop.name) || SKIP_QUALIFIED.has(`${doc.name}.${prop.name}`)) continue
    const options = enumOptions(prop.type)
    if (options && options.length <= 8) {
      for (const o of options) out.push({ label: `${prop.name} · ${o}`, prop: prop.name, props: { ...base, [prop.name]: o } })
    } else if (prop.type === `boolean` && prop.default !== true && base[prop.name] !== true) {
      out.push({ label: prop.name, prop: prop.name, props: { ...base, [prop.name]: true } })
    }
  }
  return out.slice(0, max)
}
