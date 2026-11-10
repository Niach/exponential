/* The data of one genUI component page, shared by the page generator
   (scripts/reference/eui-components.ts) and EuiSpecimen.astro: the catalog
   doc, the specimen tree, the variants gallery (a port of the old /sdk/genui/
   site's VariantGallery, minus the cases the specimen already shows) and
   the playground link. */
import { existsSync, readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { deflateRawSync } from "node:zlib"
import componentsDoc from "@exponential_app/eui/docs/components.generated.json"
import specimensDoc from "@exponential_app/eui/fixtures/specimens.json"
import samplesDoc from "@exponential_app/eui/samples/samples.json"
import type { NestedNode } from "@exponential_app/eui"
import { EUI_GROUP_LABELS, EUI_GROUP_ORDER, euiComponentSlug } from "../../../lib/eui-components.ts"

export type ComponentDoc = (typeof componentsDoc.components)[number]

export const COMPONENT_DOCS: readonly ComponentDoc[] = componentsDoc.components
const SPECIMENS = specimensDoc.specimens as unknown as { id: string; component: string; title: string; node: NestedNode }[]

/* The catalogue (#EXP-1779: /sdk/genui/ renders every component, the one
   component catalogue since the styleguide app retired): the components by
   group, in the site's group order. */
export const CATALOGUE: readonly { group: string; label: string; docs: readonly ComponentDoc[] }[] = EUI_GROUP_ORDER.map((group) => ({
  group,
  label: EUI_GROUP_LABELS[group] ?? group,
  docs: COMPONENT_DOCS.filter((d) => d.group === group),
})).filter((g) => g.docs.length > 0)

/* The specimens' example media (`https://example.com/*`, served by nobody)
   → the committed samples (packages/exponential-ui/samples, EXP-1654) as
   data: urls, so a build-time surface paints the same picture on every host
   with nothing to serve; an example no sample covers (audio, video) → a
   tenth of a second of silence. */
/** packages/exponential-ui/samples/, found by climbing from this module
 *  and from the cwd (the build runs a bundled copy of this file in dist/). */
function samplesDir(): string {
  for (let dir of [dirname(fileURLToPath(import.meta.url)), process.cwd()]) {
    for (;;) {
      const hit = join(dir, `packages/exponential-ui/samples/`)
      if (existsSync(join(hit, `samples.json`))) return hit
      const up = dirname(dir)
      if (up === dir) break
      dir = up
    }
  }
  throw new Error(`specimen: packages/exponential-ui/samples not found`)
}
const SAMPLES_DIR = samplesDir()
const SAMPLE_URLS = samplesDoc.urls as Record<string, string>
const EXAMPLE = /^https:\/\/example\.com\/[^/]+\.(png|jpe?g|mp3|mp4|wav|webm)$/
const dataUrls = new Map<string, string>()
const pngData = (file: string) => {
  let url = dataUrls.get(file)
  if (!url) dataUrls.set(file, (url = `data:image/png;base64,${readFileSync(`${SAMPLES_DIR}${file}`).toString(`base64`)}`))
  return url
}
function silenceWav(): string {
  const n = 800
  const b = Buffer.alloc(44 + n, 0x80)
  b.write(`RIFF`, 0)
  b.writeUInt32LE(36 + n, 4)
  b.write(`WAVEfmt `, 8)
  b.writeUInt32LE(16, 16)
  b.writeUInt16LE(1, 20)
  b.writeUInt16LE(1, 22)
  b.writeUInt32LE(8000, 24)
  b.writeUInt32LE(8000, 28)
  b.writeUInt16LE(1, 32)
  b.writeUInt16LE(8, 34)
  b.write(`data`, 36)
  b.writeUInt32LE(n, 40)
  return `data:audio/wav;base64,${b.toString(`base64`)}`
}
const SILENCE = silenceWav()

/** A tree with every example src swapped for its sample (see above). */
export function withSamples<T>(value: T): T {
  if (typeof value === `string`) {
    if (SAMPLE_URLS[value]) return pngData(SAMPLE_URLS[value]!) as T
    const m = EXAMPLE.exec(value)
    return (m ? (/^(png|jpe?g)$/.test(m[1]!) ? pngData(`preview.png`) : SILENCE) : value) as T
  }
  if (Array.isArray(value)) return value.map(withSamples) as T
  if (value && typeof value === `object`) return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, withSamples(v)])) as T
  return value
}

export const componentBySlug = (slug: string) => COMPONENT_DOCS.find((d) => euiComponentSlug(d) === slug)
export const specimenFor = (doc: ComponentDoc) => SPECIMENS.find((s) => s.id === doc.specimenId)?.node

// ---------------------------------------------------------------------------
// Variants (old site: lib/catalog.ts variantsOf + sdk/ComponentStudio.tsx)
// ---------------------------------------------------------------------------

const ENUM = /^[A-Za-z0-9_-]+(\s*\|\s*[A-Za-z0-9_-]+)+$/
const enumOptions = (type: string): string[] | null => (ENUM.test(type.trim()) ? type.split(`|`).map((s) => s.trim()) : null)
const RESERVED = new Set([`id`, `component`, `children`, `slots`, `on`, `style`, `accessibility`])
/* Props that change behaviour (looks stay the same) stay out of the gallery. */
const SKIP_PROPS = new Set([`open`, `pressable`, `multiple`, `autoplay`, `loop`, `dragToDismiss`, `submit`, `searchable`, `autosize`, `stickyHeaders`, `stickyHeader`, `live`, `validateOn`, `openOn`, `collapseBelow`, `gap`, `justify`, `align`, `selectable`, `side`, `external`])
const SKIP_QUALIFIED = new Set([`Input.type`, `Image.loading`])

/** The subject of a specimen's first case (the case stack = [caption, subject]). */
function subjectOf(doc: ComponentDoc, specimen: NestedNode | undefined): NestedNode | null {
  const first = specimen?.children?.[0]
  if (!first) return null
  const hit = (first.children ?? []).find((c) => c.component === doc.name && !c.id.endsWith(`-label`))
  if (hit) return hit
  if (first.component === doc.name) return first
  const queue: NestedNode[] = [...(first.children ?? [])]
  while (queue.length) {
    const n = queue.shift()!
    if (n.component === doc.name) return n
    queue.push(...(n.children ?? []), ...Object.values(n.slots ?? {}))
  }
  return null
}

/** The specimen's case labels (`variant · secondary`, …). */
function caseLabels(specimen: NestedNode | undefined): Set<string> {
  const out = new Set<string>()
  for (const c of specimen?.children ?? []) {
    const text = c.children?.[0]?.props?.text
    if (typeof text === `string`) out.add(text)
  }
  return out
}

/** A copy of `node` with every id prefixed (one surface holds every variant). */
function reId(node: NestedNode, prefix: string): NestedNode {
  const out: NestedNode = { ...node, id: `${prefix}${node.id}` }
  if (node.children) out.children = node.children.map((c) => reId(c, prefix))
  if (node.slots) out.slots = Object.fromEntries(Object.entries(node.slots).map(([k, v]) => [k, reId(v, prefix)]))
  return out
}

export interface Variant {
  label: string
  node: NestedNode
}

/** Every option of every short enum prop and every off-by-default boolean,
 *  over the specimen subject's props; the ones the specimen shows as a case
 *  (same label) are left out. Overlays get none. */
export function variantsOf(doc: ComponentDoc): Variant[] {
  if (doc.group === `overlay`) return []
  const specimen = specimenFor(doc)
  const subject = subjectOf(doc, specimen)
  const example = doc.example as Record<string, unknown>
  const base: Record<string, unknown> = {
    ...Object.fromEntries(Object.entries(example).filter(([k, v]) => !RESERVED.has(k) && v !== undefined)),
    ...(subject?.props ?? {}),
  }
  if (doc.props.some((p) => p.name === `open`)) base.open = false
  const shown = caseLabels(specimen)
  const node = (props: Record<string, unknown>, i: number): NestedNode => {
    const n: NestedNode = { id: `example`, component: doc.name, props }
    const style = (subject?.style ?? example.style) as Record<string, unknown> | undefined
    if (style) n.style = style
    if (subject?.children?.length) n.children = subject.children
    if (subject?.slots) n.slots = subject.slots
    return reId(n, `v${i}-`)
  }
  const out: Variant[] = []
  for (const p of doc.props as { name: string; type: string; default?: unknown }[]) {
    if (SKIP_PROPS.has(p.name) || SKIP_QUALIFIED.has(`${doc.name}.${p.name}`)) continue
    const options = enumOptions(p.type)
    const picks: [string, unknown][] = options && options.length <= 8 ? options.map((o) => [`${p.name} · ${o}`, o]) : p.type === `boolean` && p.default !== true && base[p.name] !== true ? [[p.name, true]] : []
    for (const [label, value] of picks) {
      if (shown.has(label) || shown.has(`${label} · true`) || base[p.name] === value) continue
      out.push({ label, node: node({ ...base, [p.name]: value }, out.length) })
    }
  }
  return out
}

/** The variants as ONE surface: a grid of captioned cells. */
export function variantsTree(doc: ComponentDoc): NestedNode | null {
  const variants = variantsOf(doc)
  if (!variants.length) return null
  return {
    id: `variants`,
    component: `Grid`,
    props: { columns: { base: 1, md: 2, lg: 3 }, gap: `lg` },
    children: variants.map((v, i) => ({
      id: `cell-${i}`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `xs`, align: `start` },
      children: [{ id: `cap-${i}`, component: `Text`, props: { text: v.label, variant: `caption` } }, v.node],
    })),
  }
}

// ---------------------------------------------------------------------------
// Playground link (the encoding of apps/marketing/ui/src/sdk/share.ts:
// JSON → deflate-raw → base64url in `#s=`)
// ---------------------------------------------------------------------------

export const PLAYGROUND_PATH = `/docs/sdk/genui/tools/playground/`

export function playgroundHref(tree: NestedNode): string {
  const state = { v: 1, source: JSON.stringify(tree, null, 2), theme: `exponential`, mode: `dark` }
  const token = deflateRawSync(Buffer.from(JSON.stringify(state))).toString(`base64url`)
  return `${PLAYGROUND_PATH}#s=${token}`
}
