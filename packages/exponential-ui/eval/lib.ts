// VAPP-108: the ergonomics eval, the pure half. A corpus prompt + the model's
// recorded reply (eval/fixtures/<id>.json) → one score row: what the
// validator said, how many tokens it cost, and a visual rubric computed on
// the AUTHORED surface (spacing rhythm, alignment, nesting depth, one primary
// action, empty / loading states). score.ts aggregates the rows into
// eval/report.json; render.ts adds what the web render measured.

import { CORE_CATALOG_ID } from "../src/catalog"
import { reduceSurface } from "../src/reducer"
import { coreCatalog } from "../src/catalog"
import type { FlatComponent, ReduceIssue, UiNode } from "../src/types"

export interface CorpusEntry {
  id: string
  title: string
  prompt: string
  /** The app screen it mirrors (views.json id), for the spacing parity. */
  appView?: string
  /** The web viewports it renders at; default [1280]. */
  widths?: number[]
  expects: Partial<Record<Rubric, boolean>>
}

export interface Corpus {
  prompts: CorpusEntry[]
}

export type Rubric = `primaryAction` | `emptyState` | `loading` | `form` | `list` | `overlay`
export const RUBRICS: readonly Rubric[] = [`primaryAction`, `emptyState`, `loading`, `form`, `list`, `overlay`]

export interface FixtureUsage {
  inputTokens: number
  outputTokens: number
  cacheReadInputTokens: number
  /** True when no API counted them (chars / 4). */
  estimated: boolean
}

export interface EvalFixture {
  id: string
  prompt: string
  model: string
  promptVariant: `full` | `terse` | `lite` | `lite-terse` | `index`
  recordedAt: string
  /** `api` = eval/record.ts over the Anthropic API (the SDK, ANTHROPIC_API_KEY);
   *  `claude-cli` = eval/record.ts through `claude -p` on the machine's Claude
   *  login (VAPP-127: the same system prompt, model and effort, real usage that
   *  includes ≈ 0.5k tokens of CLI context); `session` = the session's model
   *  from the same prompt, usage estimated. */
  recordedBy: `api` | `claude-cli` | `session`
  usage: FixtureUsage
  /** The reply verbatim; the messages are parsed from it at score time. */
  text: string
}

export interface A2uiMessage {
  version?: string
  createSurface?: { surfaceId: string; catalogId: string }
  updateComponents?: { surfaceId: string; components: FlatComponent[] }
  updateDataModel?: { surfaceId: string; path?: string; value?: unknown }
  [key: string]: unknown
}

export interface ParsedReply {
  messages: A2uiMessage[]
  /** Lines (or blocks) that were not JSON, with the reason. */
  parseErrors: string[]
}

/** The reply text → A2UI messages. Accepts JSONL (one message a line, the
 *  format the prompt asks for), a fenced ```json block holding JSONL or a
 *  JSON array, or one bare JSON array / object. Prose around them is
 *  reported, never fatal. */
export function parseReply(text: string): ParsedReply {
  const messages: A2uiMessage[] = []
  const parseErrors: string[] = []
  const fences = [...text.matchAll(/```(?:json|jsonl)?\s*\n([\s\S]*?)```/g)].map((m) => m[1])
  const bodies = fences.length ? fences : [text]
  const push = (value: unknown, where: string) => {
    if (Array.isArray(value)) value.forEach((v, i) => push(v, `${where}[${i}]`))
    else if (value && typeof value === `object`) messages.push(value as A2uiMessage)
    else parseErrors.push(`${where}: not a message`)
  }
  for (const body of bodies) {
    const trimmed = body.trim()
    if (!trimmed) continue
    // One JSON document (an array or an object spanning lines)?
    try {
      push(JSON.parse(trimmed), `document`)
      continue
    } catch {
      /* JSONL or prose */
    }
    for (const [i, line] of trimmed.split(`\n`).entries()) {
      const l = line.trim()
      if (!l) continue
      try {
        push(JSON.parse(l), `line ${i + 1}`)
      } catch {
        parseErrors.push(`line ${i + 1}: ${l.slice(0, 60)}${l.length > 60 ? `…` : ``}`)
      }
    }
  }
  return { messages, parseErrors }
}

export interface AuthoredSurface {
  surfaceId: string
  catalogId: string
  /** The last definition of every id, in first-seen order. */
  components: FlatComponent[]
  /** Every updateDataModel write, in order. */
  data: { path: string; value: unknown }[]
}

/** The messages → the surface they describe (the first createSurface's
 *  id, or the first updateComponents' when none). */
export function surfaceOf(messages: A2uiMessage[]): AuthoredSurface {
  const create = messages.find((m) => m.createSurface)?.createSurface
  const surfaceId = create?.surfaceId ?? messages.find((m) => m.updateComponents)?.updateComponents?.surfaceId ?? `main`
  const byId = new Map<string, FlatComponent>()
  for (const m of messages) {
    if (m.updateComponents?.surfaceId !== surfaceId) continue
    for (const c of m.updateComponents.components) byId.set(c.id, c)
  }
  const data = messages.filter((m) => m.updateDataModel?.surfaceId === surfaceId).map((m) => ({ path: m.updateDataModel!.path ?? ``, value: m.updateDataModel!.value }))
  return { surfaceId, catalogId: create?.catalogId ?? CORE_CATALOG_ID, components: [...byId.values()], data }
}

export interface SpacingStats {
  /** Spacing values on authored styles (padding*, margin*, gap): tokens vs raw px. */
  tokenised: number
  raw: number
  /** The distinct raw px values used (a long list = no rhythm). */
  rawValues: number[]
  /** Share of spacing values on the theme's scale: 1 = every one a $spacing token. */
  rhythm: number
}

export interface RubricScores {
  /** Buttons whose variant is the primary one (unset or `default`). */
  primaryActions: number
  emptyStates: number
  loadingStates: number
  /** Form fields (Input, Select, …) and how many sit inside a Form. */
  fields: number
  fieldsInForm: number
  lists: number
  overlays: number
  /** Horizontal Stacks whose children are controls but carry no `align` (they stretch). */
  unalignedRows: number
  /** Authored Box components (the rule says: layout macros first). */
  boxes: number
  /** Markdown components used for what a component would draw (a list or a heading). */
  markdownForUi: number
  hexColours: number
}

export interface ScoreRow {
  id: string
  title: string
  model: string
  promptVariant: EvalFixture[`promptVariant`]
  recordedBy: EvalFixture[`recordedBy`]
  usage: FixtureUsage
  parseErrors: number
  messages: number
  components: number
  /** Nesting depth of the AUTHORED tree (ids followed, templates included). */
  depth: number
  /** Validator issues (the reducer never throws). */
  issues: ReduceIssue[]
  spacing: SpacingStats
  rubric: RubricScores
  /** Per expected rubric: did the surface deliver it? */
  expects: Partial<Record<Rubric, boolean>>
  /** What the rubric found wrong, in words. */
  findings: string[]
  /** Which components the author used, with counts (the vocabulary the corpus wants). */
  vocabulary: Record<string, number>
}

const FIELD_COMPONENTS = new Set([`Input`, `Textarea`, `NumberField`, `Checkbox`, `Radio`, `ChoiceTiles`, `Switch`, `Slider`, `Rating`, `Select`, `ChipInput`, `DatePicker`, `DateRangePicker`, `TimePicker`, `FileUpload`])
const OVERLAY_COMPONENTS = new Set([`Dialog`, `Drawer`, `AlertDialog`, `Menu`, `Popover`])
const CONTROL_COMPONENTS = new Set([`Button`, `Input`, `Select`, `Badge`, `Chip`, `Avatar`, `Icon`, `Text`, `Switch`, `Checkbox`, ...FIELD_COMPONENTS])
const SPACING_KEYS = /^(padding|margin|gap|rowGap|columnGap)/

function isPrimaryButton(c: FlatComponent): boolean {
  return c.component === `Button` && (c.variant === undefined || c.variant === `default`)
}

/** The authored tree's depth: ids followed through children and slots, a
 *  template counting as one level under its owner. */
export function authoredDepth(components: readonly FlatComponent[]): number {
  const byId = new Map(components.map((c) => [c.id, c]))
  const seen = new Set<string>()
  const walk = (id: string, depth: number): number => {
    const c = byId.get(id)
    if (!c || seen.has(id)) return depth
    seen.add(id)
    let max = depth
    const kids = Array.isArray(c.children) ? c.children : c.children ? [c.children.componentId] : []
    for (const k of kids) max = Math.max(max, walk(k, depth + 1))
    for (const s of Object.values(c.slots ?? {})) max = Math.max(max, walk(s, depth + 1))
    return max
  }
  return byId.has(`root`) ? walk(`root`, 1) : 0
}

export function spacingStats(components: readonly FlatComponent[]): SpacingStats {
  let tokenised = 0
  const raw: number[] = []
  const visit = (style: unknown) => {
    if (!style || typeof style !== `object`) return
    for (const [k, v] of Object.entries(style as Record<string, unknown>)) {
      if (k.startsWith(`@`) || k.startsWith(`:`)) {
        visit(v)
        continue
      }
      if (!SPACING_KEYS.test(k)) continue
      if (typeof v === `string` && v.startsWith(`$spacing.`)) tokenised++
      else if (typeof v === `number`) raw.push(v)
      else if (typeof v === `string` && /^\d+(px)?$/.test(v)) raw.push(Number.parseInt(v, 10))
    }
  }
  for (const c of components) visit(c.style)
  const all = tokenised + raw.length
  return { tokenised, raw: raw.length, rawValues: [...new Set(raw)].sort((a, b) => a - b), rhythm: all ? tokenised / all : 1 }
}

function hasHex(value: unknown): boolean {
  if (typeof value === `string`) return /#[0-9a-f]{3,8}\b/i.test(value)
  if (value && typeof value === `object`) return Object.values(value as Record<string, unknown>).some(hasHex)
  return false
}

export function rubricScores(components: readonly FlatComponent[]): RubricScores {
  const byId = new Map(components.map((c) => [c.id, c]))
  const parentOf = new Map<string, string>()
  for (const c of components) {
    const kids = Array.isArray(c.children) ? c.children : c.children ? [c.children.componentId] : []
    for (const k of kids) parentOf.set(k, c.id)
    for (const s of Object.values(c.slots ?? {})) parentOf.set(s, c.id)
  }
  const insideForm = (id: string): boolean => {
    for (let p = parentOf.get(id); p; p = parentOf.get(p)) if (byId.get(p)?.component === `Form`) return true
    return false
  }
  const r: RubricScores = { primaryActions: 0, emptyStates: 0, loadingStates: 0, fields: 0, fieldsInForm: 0, lists: 0, overlays: 0, unalignedRows: 0, boxes: 0, markdownForUi: 0, hexColours: 0 }
  for (const c of components) {
    if (isPrimaryButton(c)) r.primaryActions++
    if (c.component === `EmptyState` || (c.component === `Table` && c.emptyText !== undefined)) r.emptyStates++
    if (c.component === `Skeleton` || c.component === `Spinner` || c.loading !== undefined || (c.component === `TraceItem` && c.status === `running`) || (c.component === `Text` && c.shimmer)) r.loadingStates++
    if (FIELD_COMPONENTS.has(c.component)) {
      r.fields++
      if (insideForm(c.id)) r.fieldsInForm++
    }
    if ((c.component === `List` || c.component === `Table` || c.component === `Grid`) && (c.component === `Table` || (c.children && !Array.isArray(c.children)))) r.lists++
    if (OVERLAY_COMPONENTS.has(c.component)) r.overlays++
    if (c.component === `Stack` && c.direction === `horizontal` && c.align === undefined && Array.isArray(c.children)) {
      const kids = c.children.map((k) => byId.get(k)?.component ?? ``)
      if (kids.length > 1 && kids.every((k) => CONTROL_COMPONENTS.has(k)) && !kids.every((k) => k === `Button`)) r.unalignedRows++
    }
    if (c.component === `Box`) r.boxes++
    if (c.component === `Markdown` && typeof c.text === `string` && /^(\s*[-*] |#)/m.test(c.text)) r.markdownForUi++
    if (hasHex(c.style) || hasHex(c.dot) || hasHex(c.backgroundColor)) r.hexColours++
  }
  return r
}

export function scoreFixture(entry: CorpusEntry, fixture: EvalFixture): ScoreRow {
  const { messages, parseErrors } = parseReply(fixture.text)
  const surface = surfaceOf(messages)
  const { issues } = reduceSurface(surface.components, { catalogId: surface.catalogId })
  const rubric = rubricScores(surface.components)
  const spacing = spacingStats(surface.components)
  const vocabulary: Record<string, number> = {}
  for (const c of surface.components) vocabulary[c.component] = (vocabulary[c.component] ?? 0) + 1
  const expects: Partial<Record<Rubric, boolean>> = {}
  const findings: string[] = []
  if (entry.expects.primaryAction) {
    expects.primaryAction = rubric.primaryActions === 1
    if (rubric.primaryActions === 0) findings.push(`no primary action`)
    if (rubric.primaryActions > 1) findings.push(`${rubric.primaryActions} primary buttons (one expected)`)
  } else if (rubric.primaryActions > 1) findings.push(`${rubric.primaryActions} primary buttons`)
  if (entry.expects.emptyState) {
    expects.emptyState = rubric.emptyStates > 0
    if (!expects.emptyState) findings.push(`no empty state`)
  }
  if (entry.expects.loading) {
    expects.loading = rubric.loadingStates > 0
    if (!expects.loading) findings.push(`no loading state`)
  }
  if (entry.expects.form) {
    expects.form = rubric.fields > 0 && rubric.fieldsInForm === rubric.fields
    if (rubric.fields === 0) findings.push(`no form fields`)
    else if (rubric.fieldsInForm < rubric.fields) findings.push(`${rubric.fields - rubric.fieldsInForm} of ${rubric.fields} fields outside a Form`)
  }
  if (entry.expects.list) {
    expects.list = rubric.lists > 0
    if (!expects.list) findings.push(`no List / Table over a data template`)
  }
  if (entry.expects.overlay) {
    expects.overlay = rubric.overlays > 0
    if (!expects.overlay) findings.push(`no overlay`)
  }
  if (rubric.unalignedRows) findings.push(`${rubric.unalignedRows} horizontal Stack(s) of controls without align`)
  if (spacing.raw) findings.push(`${spacing.raw} raw px spacing value(s): ${spacing.rawValues.join(`, `)}`)
  if (rubric.hexColours) findings.push(`${rubric.hexColours} #hex colour(s)`)
  if (rubric.markdownForUi) findings.push(`Markdown used for a list or heading ×${rubric.markdownForUi}`)
  if (parseErrors.length) findings.push(`${parseErrors.length} unparsable line(s)`)
  if (!surface.components.some((c) => c.id === `root`)) findings.push(`no root component`)
  return {
    id: entry.id,
    title: entry.title,
    model: fixture.model,
    promptVariant: fixture.promptVariant,
    recordedBy: fixture.recordedBy,
    usage: fixture.usage,
    parseErrors: parseErrors.length,
    messages: messages.length,
    components: surface.components.length,
    depth: authoredDepth(surface.components),
    issues,
    spacing,
    rubric,
    expects,
    findings,
    vocabulary,
  }
}

export interface Aggregate {
  surfaces: number
  recordedBy: Record<string, number>
  /** Surfaces the validator accepted without an issue. */
  validatorClean: number
  issuesTotal: number
  /** Issue messages by frequency (the mistakes the prompt should pre-empt). */
  issueKinds: { message: string; count: number }[]
  components: { total: number; mean: number; max: number }
  depth: { mean: number; max: number }
  tokens: { input: number; output: number; meanOutput: number; estimated: boolean }
  /** Pass rate per rubric over the surfaces that expected it. */
  rubric: Record<Rubric, { expected: number; passed: number }>
  spacing: { rhythm: number; rawValues: number[]; surfacesWithRawPx: number }
  unalignedRows: number
  boxes: number
  /** Component → surfaces using it, most used first. */
  vocabulary: { component: string; surfaces: number; uses: number }[]
  /** Offered components no surface used. */
  unused: string[]
}

export function aggregate(rows: readonly ScoreRow[]): Aggregate {
  const rubric = Object.fromEntries(RUBRICS.map((r) => [r, { expected: 0, passed: 0 }])) as Aggregate[`rubric`]
  const issueKinds = new Map<string, number>()
  const recordedBy: Record<string, number> = {}
  const vocab = new Map<string, { surfaces: number; uses: number }>()
  let rawTotal = 0
  let tokenTotal = 0
  const rawValues = new Set<number>()
  for (const row of rows) {
    recordedBy[row.recordedBy] = (recordedBy[row.recordedBy] ?? 0) + 1
    for (const issue of row.issues) {
      const kind = issue.message.replace(/`[^`]*`/g, `…`).replace(/\b\d+\b/g, `N`)
      issueKinds.set(kind, (issueKinds.get(kind) ?? 0) + 1)
    }
    for (const [r, ok] of Object.entries(row.expects) as [Rubric, boolean][]) {
      rubric[r].expected++
      if (ok) rubric[r].passed++
    }
    rawTotal += row.spacing.raw
    tokenTotal += row.spacing.tokenised
    row.spacing.rawValues.forEach((v) => rawValues.add(v))
    for (const [c, n] of Object.entries(row.vocabulary)) {
      const v = vocab.get(c) ?? { surfaces: 0, uses: 0 }
      v.surfaces++
      v.uses += n
      vocab.set(c, v)
    }
  }
  const offered = Object.entries(coreCatalog.components).filter(([, d]) => !d.hidden).map(([n]) => n)
  const mean = (xs: number[]) => (xs.length ? Math.round((xs.reduce((a, b) => a + b, 0) / xs.length) * 10) / 10 : 0)
  return {
    surfaces: rows.length,
    recordedBy,
    validatorClean: rows.filter((r) => r.issues.length === 0).length,
    issuesTotal: rows.reduce((a, r) => a + r.issues.length, 0),
    issueKinds: [...issueKinds.entries()].map(([message, count]) => ({ message, count })).sort((a, b) => b.count - a.count || a.message.localeCompare(b.message)),
    components: { total: rows.reduce((a, r) => a + r.components, 0), mean: mean(rows.map((r) => r.components)), max: Math.max(0, ...rows.map((r) => r.components)) },
    depth: { mean: mean(rows.map((r) => r.depth)), max: Math.max(0, ...rows.map((r) => r.depth)) },
    tokens: {
      input: rows.reduce((a, r) => a + r.usage.inputTokens, 0),
      output: rows.reduce((a, r) => a + r.usage.outputTokens, 0),
      meanOutput: mean(rows.map((r) => r.usage.outputTokens)),
      estimated: rows.some((r) => r.usage.estimated),
    },
    rubric,
    spacing: { rhythm: rawTotal + tokenTotal ? Math.round((tokenTotal / (rawTotal + tokenTotal)) * 1000) / 1000 : 1, rawValues: [...rawValues].sort((a, b) => a - b), surfacesWithRawPx: rows.filter((r) => r.spacing.raw > 0).length },
    unalignedRows: rows.reduce((a, r) => a + r.rubric.unalignedRows, 0),
    boxes: rows.reduce((a, r) => a + r.rubric.boxes, 0),
    vocabulary: [...vocab.entries()].map(([component, v]) => ({ component, ...v })).sort((a, b) => b.surfaces - a.surfaces || b.uses - a.uses || a.component.localeCompare(b.component)),
    unused: offered.filter((n) => !vocab.has(n)),
  }
}

/** The corpus entry's web viewports. */
export const widthsOf = (entry: CorpusEntry): number[] => entry.widths ?? [1280]

/** The authored surface as ONE nested tree + its data model: what every
 *  renderer is handed (the web harnesses' `?view=tree`, render.ts; the
 *  natives' eval specimens, eval-specimens.json). Template nodes ride along under
 *  their owner as `template` children (what reduceNested lifts back out). */
export function nestedSurface(surface: AuthoredSurface): { tree: unknown; data: Record<string, unknown> } {
  const { root, templates } = reduceSurface(surface.components, { catalogId: CORE_CATALOG_ID, expand: false })
  return { tree: withTemplates(root, templates ?? {}), data: dataModel(surface.data) }
}

function withTemplates(node: UiNode, templates: Record<string, UiNode>): unknown {
  const out: Record<string, unknown> = { id: node.id, component: node.component, props: node.props }
  if (node.style) out.style = node.style
  if (node.on) out.on = node.on
  if (node.visible !== undefined) out.visible = node.visible
  if (node.accessibility) out.accessibility = node.accessibility
  if (node.template) {
    out.template = node.template
    const t = templates[node.template.component]
    if (t) out.children = [withTemplates(t, templates)]
  } else if (node.children.length) out.children = node.children.map((c) => withTemplates(c, templates))
  if (node.slots) out.slots = Object.fromEntries(Object.entries(node.slots).map(([k, v]) => [k, withTemplates(v, templates)]))
  return out
}

/** The updateDataModel writes, in order, as one data model. */
export function dataModel(writes: { path: string; value: unknown }[]): Record<string, unknown> {
  const data: Record<string, unknown> = {}
  for (const w of writes) {
    const parts = w.path.split(`/`).filter(Boolean)
    if (!parts.length) {
      Object.assign(data, w.value as Record<string, unknown>)
      continue
    }
    let cursor: Record<string, unknown> = data
    for (const p of parts.slice(0, -1)) cursor = (cursor[p] ??= {}) as Record<string, unknown>
    cursor[parts[parts.length - 1]] = w.value
  }
  return data
}

/** VAPP-127: the native view id of a corpus surface (the example apps' `-shot` /
 *  `shot` extra, the IDE's `EXP_DEV_SCREEN`). */
export const EVAL_SPECIMEN_PREFIX = `exponential-ui-eval-`
export const evalSpecimenId = (corpusId: string) => `${EVAL_SPECIMEN_PREFIX}${corpusId}`

export interface EvalSpecimen {
  id: string
  corpusId: string
  title: string
  node: unknown
  data: Record<string, unknown>
}

/** Every recorded surface as a native specimen: the same tree + data the web
 *  render hands the harnesses (eval/eval-specimens.json, generated by score.ts). */
export function evalSpecimens(corpus: Corpus, fixtureOf: (id: string) => EvalFixture | undefined): EvalSpecimen[] {
  return corpus.prompts.flatMap((entry) => {
    const fixture = fixtureOf(entry.id)
    if (!fixture) return []
    const { tree, data } = nestedSurface(surfaceOf(parseReply(fixture.text).messages))
    return [{ id: evalSpecimenId(entry.id), corpusId: entry.id, title: entry.title, node: tree, data }]
  })
}
