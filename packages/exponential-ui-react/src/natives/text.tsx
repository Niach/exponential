// VAPP-87 + round 1: Text (`live` → an aria-live region; EXP-1324 `shimmer`
// → `data-xui-shimmer`, base-css), Markdown (`lines` truncation), Icon.

import { displayString } from "@exponential_app/eui"
import { useLayoutEffect, useRef, useState } from "react"
import { liveOf, propAttribute, textLinkSegments, watchMiddleTruncate } from "@exponential_app/eui/web"
import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import { BuiltinMarkdown } from "../markdown"
import type { NativeProps } from "../node-view"
import { mergeStyle } from "../node-view"
import { num, str } from "./shared"

/** #EXP-1674: a host's text links: the text's own colour, underlined; a tap
 *  opens through the URL policy and never presses the Row behind it. */
export const TEXT_LINK_STYLE = { color: `inherit`, textDecoration: `underline` } as const

export function TextNative({ props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const segments = props.hostLinks === true ? textLinkSegments(ctx.host, displayString(props.text)) : null
  // #EXP-1778: `truncate` keeps one line; `middle` shows start…end.
  const truncate = props.truncate === `middle` ? `middle` : props.truncate === `end` ? `end` : undefined
  const lines = truncate ? 1 : num(props.lines, 0)
  const full = displayString(props.text)
  const middle = useMiddleTruncate(truncate === `middle` && !segments ? full : null)
  const align = props.align === `end` ? `end` : props.align === `center` ? `center` : props.align === `start` ? `start` : undefined
  // `align` is a base-layer rule (`data-xui-align`), so a node style
  // `textAlign` (the node layer) wins over it (round 2 §2, text-direction.json).
  const style: Record<string, string | number> = {}
  if (lines > 1) style.WebkitLineClamp = lines
  // Round 9 §5.1: `accessibility.live` (already on the root) wins over `live`.
  const live = liveOf(rootProps as Record<string, unknown>, props.live)
  return (
    <span {...(rootProps as Record<string, unknown>)} ref={middle.ref} data-truncate={truncate} aria-live={live} aria-atomic={live && live !== `off` ? true : undefined} data-xui-align={align} data-lines={lines > 0 ? lines : undefined} data-clamp={lines > 1 ? `` : undefined} data-xui-shimmer={props.shimmer === true ? `` : undefined} style={mergeStyle(rootProps, Object.keys(style).length ? style : undefined)}>
      {segments
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
  return (
    <div {...(rootProps as Record<string, unknown>)} {...(ctx.host.markdownOptions?.density ? { [propAttribute(false, `density`)]: ctx.host.markdownOptions.density } : {})} data-lines={lines > 0 ? lines : undefined} style={mergeStyle(rootProps, lines > 0 ? ({ WebkitLineClamp: lines } as React.CSSProperties) : undefined)}>
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
