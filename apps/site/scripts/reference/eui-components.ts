/* genUI's component reference: one page per catalog component plus the index,
   GENERATED from @exponential_app/eui's data (docs/components.generated.json,
   docs/themes.generated.json, catalog/macros.json, fixtures/specimens.json).
   The page body is markdown so llms.txt carries every table; the live render
   is EuiSpecimen (build time, zero JS), the pictures come from shots/. */
import { existsSync } from "node:fs"
import { join, resolve } from "node:path"
import themesDoc from "@exponential_app/eui/docs/themes.generated.json"
import macrosDoc from "@exponential_app/eui/catalog/macros.json"
import { EUI_GROUP_LABELS, EUI_GROUP_ORDER, euiComponentPath, euiComponentSlug } from "../../src/lib/eui-components.ts"
import { COMPONENT_DOCS, playgroundHref, specimenFor, variantsOf, type ComponentDoc } from "../../src/components/eui/components/specimen.ts"
import type { GeneratedPage } from "../sync-reference.ts"

const SHOTS = resolve(import.meta.dir, `../../../../shots`)
const DIR = `docs/sdk/genui/reference/components`
const COMPONENTS = `../../../../../../../components`
const EDIT = `https://github.com/Niach/exponential/edit/master/packages/exponential-ui/catalog/core.catalog.json`
const THEMES_PAGE = `/docs/sdk/genui/reference/themes/`
const THEMES_GUIDE = `/docs/sdk/genui/guides/themes/`

// ---------------------------------------------------------------------------
// Text: catalog sentences into MDX
// ---------------------------------------------------------------------------

/** Plain text safe in MDX (and in a table cell). */
const escapeText = (text: string) =>
  text
    .replace(/\\/g, `\\\\`)
    .replace(/[{}*_[\]|]/g, (c) => `\\${c}`)
    .replace(/</g, `&lt;`)
    .replace(/>/g, `&gt;`)

/** Inline code (literal in MDX); a `|` escaped for table cells. */
const code = (text: string) => `\`${text.replace(/\|/g, `\\|`)}\``

/** A catalog sentence: its `backticked` words as code, the rest escaped. */
const rich = (text: string) =>
  text
    .split(/(`[^`]+`)/g)
    .map((p) => (p.length > 2 && p.startsWith(`\``) && p.endsWith(`\``) ? code(p.slice(1, -1)) : escapeText(p)))
    .join(``)
    .replace(/\s*\n\s*/g, ` `)

const SUMMARY_MAX = 160

/** The page's sentence: the catalog description, cut at a clause boundary
 *  past 160 chars (the old site's componentSummary). */
function summary(doc: ComponentDoc): string {
  const text = doc.description.trim()
  if (text.length <= SUMMARY_MAX) return text
  const head = text.slice(0, SUMMARY_MAX - 1)
  for (const sep of [`; `, `, `, ` `]) {
    const at = head.lastIndexOf(sep)
    if (at > 40) return `${head.slice(0, at).replace(/[,;:\s]+$/, ``)}.`
  }
  return `${head}.`
}

/** A markdown table (llms.txt keeps it), wrapped for the column fit. */
const table = (head: string[], rows: string[][]) =>
  [`<div class="eui-ref-table">`, ``, `| ${head.join(` | `)} |`, `| ${head.map(() => `---`).join(` | `)} |`, ...rows.map((r) => `| ${r.join(` | `)} |`), ``, `</div>`].join(`\n`)

// ---------------------------------------------------------------------------
// Catalog relations (the old site's lib/catalog.ts)
// ---------------------------------------------------------------------------

const byName = (name: string) => COMPONENT_DOCS.find((d) => d.name === name)
const link = (doc: ComponentDoc) => `[${doc.name}](${euiComponentPath(doc)})`
const MACROS = (macrosDoc as { macros: Record<string, unknown> }).macros

/** The components a macro expands into, sorted. */
function macroParts(name: string): string[] {
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
  return [...out].filter((c) => byName(c)).sort()
}

/** Related: the components its sentence names, what it is built from, what
 *  is built from it, then its group. */
function related(doc: ComponentDoc, max = 8): ComponentDoc[] {
  const names = new Set<string>()
  for (const d of COMPONENT_DOCS) if (d.name !== doc.name && new RegExp(`(^|[^A-Za-z])${d.name}([^A-Za-z]|$)`).test(doc.description)) names.add(d.name)
  for (const n of macroParts(doc.name)) if (n !== `Box`) names.add(n)
  if (doc.name !== `Box` && doc.name !== `Text`) for (const d of COMPONENT_DOCS) if (d.name !== doc.name && macroParts(d.name).includes(doc.name)) names.add(d.name)
  for (const d of COMPONENT_DOCS) if (d.group === doc.group && d.name !== doc.name) names.add(d.name)
  return [...names].slice(0, max).map((n) => byName(n)!)
}

/** One keyboard line: `keys: action` (a line without one is a note). */
function keyRow(line: string): [string, string] {
  const at = line.indexOf(`: `)
  if (at < 0 || at > 48) return [`–`, rich(line)]
  const keys = line
    .slice(0, at)
    .split(/\s*(?:,|\/| or )\s*/)
    .map((k) => k.trim())
    .filter(Boolean)
  return [keys.map(code).join(` `), rich(line.slice(at + 2))]
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

const ORDERED = EUI_GROUP_ORDER.flatMap((g) => COMPONENT_DOCS.filter((d) => d.group === g))
const unordered = COMPONENT_DOCS.filter((d) => !ORDERED.includes(d))
if (unordered.length) throw new Error(`eui-components: no group order for ${unordered.map((d) => `${d.name} (${d.group})`).join(`, `)}`)

const front = (title: string, description: string, slug: string, editUrl: string | false) =>
  [`---`, `title: ${JSON.stringify(title)}`, `description: ${JSON.stringify(description)}`, `slug: ${slug}`, `editUrl: ${editUrl === false ? `false` : editUrl}`, `---`, ``].join(`\n`)

function componentPage(doc: ComponentDoc): GeneratedPage {
  const slug = euiComponentSlug(doc)
  const specimen = specimenFor(doc)
  const recipe = themesDoc.components.find((c) => c.name === doc.name)
  const parts = doc.kind === `macro` ? macroParts(doc.name) : []
  const a11y = doc.accessibility
  const hasShots = [`web`, `ios`, `android`, `desktop`].some((p) => existsSync(join(SHOTS, doc.specimenId, `${p}.webp`)))
  const out: string[] = [front(doc.name, summary(doc), `${DIR}/${slug}`, EDIT)]

  out.push(`import EuiSpecimen from "${COMPONENTS}/eui/EuiSpecimen.astro"`)
  if (hasShots) out.push(`import PlatformShots from "${COMPONENTS}/PlatformShots.astro"`)
  out.push(``, rich(summary(doc)), ``, `**Group:** ${EUI_GROUP_LABELS[doc.group] ?? doc.group} · **Kind:** ${doc.kind}${doc.lite ? ` · in the lite catalog` : ``}`, ``)

  out.push(`## Preview`, ``, `<EuiSpecimen slug="${slug}" />`, ``)
  if (specimen) out.push(`[Open in the playground](${playgroundHref(specimen)})`, ``)

  if (hasShots) out.push(`## Platforms`, ``, `The same specimen painted by React, SwiftUI, Compose and gpui.`, ``, `<PlatformShots view="${doc.specimenId}" />`, ``)

  if (variantsOf(doc).length) out.push(`## Variants`, ``, `<EuiSpecimen slug="${slug}" variants />`, ``)

  out.push(`## Props`, ``)
  if (doc.props.length === 0) out.push(`None.`, ``)
  else
    out.push(
      table(
        [`Prop`, `Type`, `Required`, `Bindable`, `Description`],
        doc.props.map((p) => {
          const def = (p as { default?: unknown }).default
          return [code(p.name), code(String(p.type)), p.required ? `yes` : `–`, p.bindable ? `yes` : `–`, `${rich(p.description)}${def === undefined ? `` : ` Default ${code(JSON.stringify(def))}.`}${p.responsive ? ` Responsive: takes a per-breakpoint object.` : ``}`]
        })
      ),
      ``
    )

  out.push(`## Events`, ``, doc.events.length ? doc.events.map((e) => `- ${code(`on.${e}`)}: runs an action`).join(`\n`) : `None.`, ``)
  out.push(`## Slots`, ``, doc.slots.length ? doc.slots.map((s) => `- ${code(s)}: one component by id`).join(`\n`) : `None.`, ``)
  out.push(`## Children`, ``, code(doc.children), ``)

  out.push(`## A2UI basic mapping`, ``, doc.basic.length ? `The basic catalog's ${doc.basic.map(code).join(`, `)} render${doc.basic.length === 1 ? `s` : ``} as ${doc.name}.` : `No basic catalog component maps here.`, ``)

  out.push(`## Built from`, ``, doc.kind === `macro` ? (parts.length ? parts.map((p) => link(byName(p)!)).join(` · `) : `A macro of ${doc.name} only.`) : `A native component: every renderer paints it directly.`, ``)

  out.push(`## Accessibility`, ``, `**Role:** ${code(a11y?.role ?? `none`)}`, ``)
  if (a11y?.notes) out.push(rich(a11y.notes), ``)
  const keys = a11y?.keys ?? []
  out.push(keys.length ? table([`Keys`, `Action`], keys.map(keyRow)) : `Not focusable; no keys.`, ``)

  out.push(`## Theming`, ``)
  if (recipe) {
    out.push(`**Recipe parts:** ${recipe.parts.map((p) => code(`${doc.name}/${p}`)).join(` `)}`, ``)
    out.push(`**Keyed by:** ${[...recipe.props.map(code), ...themesDoc.states.map((s) => code(`:${s}`))].join(` `)}`, ``)
  } else out.push(`No recipe: styled through the components it is built from.`, ``)
  out.push(`[Themes](${THEMES_PAGE}) · [Write a theme](${THEMES_GUIDE})`, ``)

  const rel = related(doc)
  if (rel.length) out.push(`## Related`, ``, rel.map(link).join(` · `), ``)

  return { file: `${DIR}/${slug}.gen.mdx`, content: out.join(`\n`) }
}

function indexPage(): GeneratedPage {
  const out: string[] = [
    front(`Components`, `Every genUI catalog component by group, each with a live render, the four platform shots, props, events, accessibility and theming.`, DIR, EDIT),
    ``,
    `${COMPONENT_DOCS.length} components in ${EUI_GROUP_ORDER.length} groups. Each page renders the component live, shows it on web, iOS, Android and desktop, and lists its props, events, slots, keyboard and recipe parts.`,
    ``,
  ]
  for (const group of EUI_GROUP_ORDER) {
    const docs = COMPONENT_DOCS.filter((d) => d.group === group)
    if (!docs.length) continue
    out.push(`## ${EUI_GROUP_LABELS[group] ?? group}`, ``, `<div class="eui-component-index">`, ``, docs.map((d) => `- ${link(d)}`).join(`\n`), ``, `</div>`, ``)
  }
  return { file: `${DIR}/index.gen.mdx`, content: out.join(`\n`) }
}

export const PAGES: GeneratedPage[] = [indexPage(), ...ORDERED.map(componentPage)]
