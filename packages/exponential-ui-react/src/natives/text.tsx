// VAPP-87 + round 1: Text (`live` → an aria-live region; EXP-1324 `shimmer`
// → `data-xui-shimmer`, base-css), Markdown (`lines` truncation), Icon.

import { displayString, resolveNodeStyle, runsText, textSpans } from "@exponential_app/eui"
import { useLayoutEffect, useMemo, useRef, useState } from "react"
import { linkHref, liveOf, paintHighlights, propAttribute, reactCss, textLinkSegments, textRunsPaint, watchMiddleTruncate, type TextRunsPaint } from "@exponential_app/eui/web"
import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import { BuiltinMarkdown } from "../markdown"
import type { NativeProps } from "../node-view"
import { mergeStyle } from "../node-view"
import { num, str } from "./shared"

/** #EXP-1674: a host's text links: the text's own colour, underlined; a tap
 *  opens through the URL policy and never presses the Row behind it. */
export const TEXT_LINK_STYLE = { color: `inherit`, textDecoration: `underline` } as const

export function TextNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  // Round 4 (fixtures/text-runs.json): `runs` replace `text`; `dropCap`.
  const spans = textSpans(props)
  const rich = useTextRuns(node, props)
  const segments = props.hostLinks === true && !spans.rich ? textLinkSegments(ctx.host, displayString(props.text)) : null
  // #EXP-1778: `truncate` keeps one line; `middle` shows start…end (with
  // runs: `end`).
  const truncate = props.truncate === `middle` ? (spans.rich ? `end` : `middle`) : props.truncate === `end` ? `end` : undefined
  const lines = truncate ? 1 : num(props.lines, 0)
  const full = spans.rich ? runsText(spans.runs) : displayString(props.text)
  const middle = useMiddleTruncate(truncate === `middle` && !segments ? full : null)
  const align = props.align === `end` ? `end` : props.align === `center` ? `center` : props.align === `start` ? `start` : undefined
  // `align` is a base-layer rule (`data-xui-align`), so a node style
  // `textAlign` (the node layer) wins over it (round 2 §2, text-direction.json).
  const style: Record<string, string | number> = {}
  if (lines > 1) style.WebkitLineClamp = lines
  // Round 4 (text-runs.json): runs and a drop cap paint start-aligned.
  if (rich) style.textAlign = `start`
  // Round 9 §5.1: `accessibility.live` (already on the root) wins over `live`.
  const live = liveOf(rootProps as Record<string, unknown>, props.live)
  // #EDE-95: `highlight` marks the search term's matches in the shown text.
  useHighlight(middle.ref, props.highlight, false, middle.shown ?? full)
  return (
    <span {...(rootProps as Record<string, unknown>)} ref={middle.ref} data-truncate={truncate} aria-live={live} aria-atomic={live && live !== `off` ? true : undefined} data-xui-align={rich ? undefined : align} data-lines={lines > 0 ? lines : undefined} data-clamp={lines > 1 ? `` : undefined} data-xui-shimmer={props.shimmer === true ? `` : undefined} data-xui-dropcap={rich?.cap ? `` : undefined} style={mergeStyle(rootProps, Object.keys(style).length ? style : undefined)}>
      {rich
        ? [
            rich.cap ? (
              <span key="cap" data-xui-cap="" style={reactCss(rich.cap.css)}>
                {rich.cap.text}
              </span>
            ) : null,
            ...rich.runs.map((run, i) => {
              const inner = run.code ? (
                <code key={i} className={run.code.class} {...run.code.attrs} style={run.href ? undefined : reactCss(run.css)}>
                  {run.text}
                </code>
              ) : (
                run.text
              )
              if (run.href)
                return (
                  <a
                    key={i}
                    href={run.href}
                    data-xui-text-link=""
                    style={{ ...TEXT_LINK_STYLE, ...reactCss(run.css) }}
                    onClick={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      ctx.openUrl(run.href!)
                    }}
                    onKeyDown={(e) => e.stopPropagation()}
                  >
                    {inner}
                  </a>
                )
              if (run.code) return inner
              return run.css.length ? (
                <span key={i} style={reactCss(run.css)}>
                  {run.text}
                </span>
              ) : (
                run.text
              )
            }),
          ]
        : segments
        ? segments.map((segment, i) =>
            segment.href ? (
              <a
                key={i}
                href={segment.href}
                data-xui-text-link=""
                style={TEXT_LINK_STYLE}
                onClick={(e) => {
                  e.preventDefault()
                  e.stopPropagation()
                  ctx.openUrl(segment.href!)
                }}
                onKeyDown={(e) => e.stopPropagation()}
              >
                {segment.text}
              </a>
            ) : (
              segment.text
            )
          )
        : (middle.shown ?? full)}
    </span>
  )
}

/** Round 4 (fixtures/text-runs.json): the Text's spans and drop cap to
 *  paint (null = its plain text). */
function useTextRuns(node: NativeProps[`node`], props: Record<string, unknown>): TextRunsPaint | null {
  const ctx = useSurfaceContext()
  return useMemo(() => {
    const style = ctx.theme ? (resolveNodeStyle(ctx.theme, node, ctx.mode) as Record<string, unknown>) : {}
    const fontSize = typeof style.fontSize === `number` ? style.fontSize : 14
    const lineHeight = typeof style.lineHeight === `number` ? style.lineHeight : 20
    return textRunsPaint({ theme: ctx.theme, scope: ctx.compiled.scope, props, fontSize, lineHeight, href: (raw) => linkHref(ctx.host, raw) })
  }, [ctx.theme, ctx.mode, ctx.compiled.scope, ctx.host, node, props])
}

/** #EDE-95: paints `highlight` (the raw prop) over the rendered text of
 *  `ref` (web/highlight.ts), again whenever `text` or the prop changes. */
export function useHighlight(ref: { current: Element | null }, highlight: unknown, markdown: boolean, text: string) {
  const key = highlight === undefined ? `` : JSON.stringify(highlight)
  useLayoutEffect(() => {
    const el = ref.current
    if (!key || !el) return
    return paintHighlights(el, highlight, markdown)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, markdown, text])
}

/** #EXP-1778: the middle-cut line of `text` in the span's box (null = off). */
function useMiddleTruncate(text: string | null) {
  const ref = useRef<HTMLSpanElement>(null)
  const [shown, setShown] = useState<string | null>(null)
  useLayoutEffect(() => {
    const el = ref.current
    if (text === null || !el) {
      setShown(null)
      return
    }
    return watchMiddleTruncate(el, text, setShown)
  }, [text])
  return { ref, shown: text === null ? null : shown }
}

export function MarkdownNative({ domId, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const text = displayString(props.text)
  const Host = ctx.host.Markdown
  const lines = num(props.lines, 0)
  // Round 9 §9.1: the inline profile is always the built-in painter.
  const inline = props.profile === `inline` ? (props.variant === `caption` ? `caption` : `body`) : undefined
  const ref = useRef<HTMLDivElement>(null)
  // #EDE-95: the search term's matches in the rendered markdown.
  useHighlight(ref, props.highlight, true, text)
  return (
    <div ref={ref} {...(rootProps as Record<string, unknown>)} {...(ctx.host.markdownOptions?.density ? { [propAttribute(false, `density`)]: ctx.host.markdownOptions.density } : {})} data-lines={lines > 0 ? lines : undefined} style={mergeStyle(rootProps, lines > 0 ? ({ WebkitLineClamp: lines } as React.CSSProperties) : undefined)}>
      {Host && !inline ? <Host text={text} className="xui-md" /> : <BuiltinMarkdown text={text} host={ctx.host} id={domId} inline={inline} />}
    </div>
  )
}

export function IconNative({ props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const name = str(props.name, `ui-icon-placeholder`)
  const label = str(props.label)
  return (
    <span {...(rootProps as Record<string, unknown>)} role={label ? `img` : undefined} aria-label={label || undefined} aria-hidden={label ? undefined : true}>
      <IconGlyph icons={ctx.host.icons} name={name} />
    </span>
  )
}
