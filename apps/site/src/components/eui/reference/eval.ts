/* The ergonomics eval data (packages/exponential-ui/eval): report.json
   (score.ts, render.ts, parity.ts) and rewrite-gaps.json, typed for the
   Eval page. Nothing is typed in by hand. */
import report from "@exponential_app/eui/eval/report.json"
import gaps from "@exponential_app/eui/eval/rewrite-gaps.json"

export { gaps, report }

// A clean eval leaves these arrays empty, so they are typed by hand.
export type Issue = { id: string; message: string }
export type IssueKind = { message: string; count: number }
export type RendererMeasure = { height: number; nodes: number; gaps: number[]; distinctGaps: number[]; textInset: number | null; overflowsX: boolean }
export type Candidate = `solid` | `element`
export const CANDIDATES: Candidate[] = [`solid`, `element`]
export const RENDERER_LABEL: Record<Candidate, string> = { solid: `Solid`, element: `Element` }
export type Measure = { width: number; react: RendererMeasure; solid: RendererMeasure; element: RendererMeasure; equal: boolean; differences: Record<Candidate, string[]>; pixelDiff: Record<Candidate, { pixels: number; share: number }> }
export type ParityRow = {
  platform: string
  native: { band: { height: number; titleX: number }; firstRow: { titleX: number; gapBelowBand: number } }
  template: { band: { height: number; titleX: number }; firstRow: { titleX: number; gapBelowBand: number } }
  delta: { bandHeight: number; bandTitleX: number; rowTitleX: number; gapBelowBand: number }
  bandDiff: { share: number }
}
export type Row = (typeof report.rows)[number]

export const aggregate = report.aggregate
export const render = report.render as unknown as Record<string, Measure[]>
export const parity = (report.parity as unknown as { rows: ParityRow[] }).rows
export const SOLID_GAP_CLASSES = gaps.renderers.solid.classes as string[]

export const evalLink = (p: string) => `https://github.com/Niach/exponential/blob/master/packages/exponential-ui/eval/${p}`
export const pct = (x: number, d: number) => (d ? `${Math.round((x / d) * 100)} %` : `–`)
/** Store pixels are 1.25 CSS px. */
export const cssPx = (storePx: number) => `${Math.round((storePx / 1.25) * 10) / 10} px`

export const RUBRIC_LABEL: Record<string, string> = {
  primaryAction: `exactly one primary action`,
  emptyState: `an empty state where a list can be empty`,
  loading: `a loading state`,
  form: `every field inside a Form`,
  list: `a List or Table over a data template`,
  overlay: `the dialog, drawer or menu asked for`,
}
export const CLASS_LABEL: Record<string, string> = {
  expressible: `Expressible today`,
  macro: `Needs a layout macro`,
  extension: `Needs an app extension component`,
  native: `Needs a new native`,
}
const RECORDED_BY: Record<string, string> = {
  api: `over the API`,
  "claude-cli": `through the Claude Code CLI (real usage)`,
}

export const recordedBy = () =>
  Object.entries(aggregate.recordedBy as Record<string, number>)
    .map(([k, v]) => `${v} ${RECORDED_BY[k] ?? `by the session's own model from the same prompt`}`)
    .join(`, `)

export const findingsOf = (r: Row) => [...(r.issues as Issue[]).map((i) => `${i.id}: ${i.message}`), ...(r.findings as string[])]

/** Tokens of the cheapest route: the index prompt plus each used component's block. */
export const indexRouteTokens = () => report.prompt.index.tokens + aggregate.vocabulary.length * report.prompt.perComponent.meanTokens

export const renderFindings = () =>
  report.rows.flatMap((r) =>
    (render[r.id] ?? []).flatMap((m) => CANDIDATES.filter((c) => m.differences[c].length).map((c) => ({ title: r.title, width: m.width, renderer: RENDERER_LABEL[c], differences: m.differences[c] })))
  )
