// Round 4 group B (fixtures/text-runs.json): what a web Text paints for its
// `runs` and `dropCap`, React and Solid alike: per span its text, CSS
// declarations, a policed link and the inline code part; the drop cap as a
// float (the natives reproduce its layout).

import { dropCapLines, dropCapSize, dropCapSplit, nativeRecipeProps, recipeStyle, textSpans, type ResolvedTheme, type TextRun } from "@exponential_app/eui"
import { markdownPartAttrs } from "./markdown-parts"
import { propColor, tokenVar } from "./theme-css"

/** One painted span. `code` = the Markdown/code part's class + attributes. */
export interface RunPaint {
  text: string
  css: [string, string][]
  href?: string
  code?: { class: string; attrs: Record<string, string> }
}

export interface TextRunsPaint {
  runs: RunPaint[]
  cap?: { text: string; css: [string, string][] }
}

export interface TextRunsInput {
  theme: ResolvedTheme | null | undefined
  scope: string
  props: Record<string, unknown>
  /** The Text's resolved type (px), for the drop cap. */
  fontSize: number
  lineHeight: number
  /** The URL policy: an allowed href, else undefined (plain text). */
  href: (raw: string) => string | undefined
}

function toneColor(theme: ResolvedTheme | null | undefined, props: Record<string, unknown>, tone: string | undefined): string | undefined {
  if (!tone || !theme) return undefined
  const style = recipeStyle(theme, { component: `Text`, part: `root`, props: nativeRecipeProps(`Text`, { ...props, tone }) }) as Record<string, unknown>
  return propColor(style.color)
}

function runCss(input: TextRunsInput, run: TextRun, link: boolean): [string, string][] {
  const css: [string, string][] = []
  if (run.weight) css.push([`font-weight`, tokenVar(`$type.weight.${run.weight}`) ?? run.weight])
  const color = toneColor(input.theme, input.props, run.tone)
  if (color) css.push([`color`, color])
  if (run.mark === `highlight`) css.push([`background-color`, `var(--xui-color-match)`])
  const lines = [run.mark === `strike` ? `line-through` : ``, run.mark === `underline` || link ? `underline` : ``].filter(Boolean)
  if (lines.length) css.push([`text-decoration-line`, lines.join(` `)], [`text-decoration-thickness`, `1px`])
  // Inline code is one unbreakable pill (the natives' code chip).
  if (run.mark === `code`) css.push([`white-space`, `nowrap`])
  return css
}

/** The Text's runs to paint, null = plain `text` (no runs, or `shimmer`)
 *  and no drop cap. */
export function textRunsPaint(input: TextRunsInput): TextRunsPaint | null {
  const { runs, rich, dropCap } = textSpans(input.props)
  if ((!rich || input.props.shimmer === true) && !dropCap) return null
  let spans = runs
  let cap: TextRunsPaint[`cap`]
  const lines = dropCapLines(input.props)
  if (lines) {
    const split = dropCapSplit(runs)
    if (split) {
      spans = split.rest
      const size = dropCapSize(input.fontSize, input.lineHeight, lines)
      const css: [string, string][] = [
        [`float`, `inline-start`],
        [`font-size`, `${size}px`],
        [`line-height`, `${lines * input.lineHeight}px`],
        [`height`, `${lines * input.lineHeight}px`],
        [`margin-inline-end`, tokenVar(`$spacing.xs`) ?? `4px`],
      ]
      if (split.cap.weight) css.push([`font-weight`, tokenVar(`$type.weight.${split.cap.weight}`) ?? split.cap.weight])
      const color = toneColor(input.theme, input.props, split.cap.tone)
      if (color) css.push([`color`, color])
      cap = { text: split.cap.text, css }
    }
  }
  return {
    runs: spans.map((run) => {
      const href = run.link ? input.href(run.link) : undefined
      return { text: run.text, css: runCss(input, run, !!href), ...(href ? { href } : {}), ...(run.mark === `code` ? { code: markdownPartAttrs(input.scope, `code`, undefined) } : {}) }
    }),
    ...(cap ? { cap } : {}),
  }
}

/** Declarations as a React style object (camelCase keys). */
export function reactCss(css: readonly [string, string][]): Record<string, string> {
  const out: Record<string, string> = {}
  for (const [k, v] of css) out[k.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase())] = v
  return out
}

/** Declarations as a Solid style object (kebab keys). */
export function solidCss(css: readonly [string, string][]): Record<string, string> {
  return Object.fromEntries(css)
}
