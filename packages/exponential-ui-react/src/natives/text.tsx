// VAPP-87 + round 1: Text (`live` → an aria-live region), Markdown (`lines`
// truncation), Icon.

import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import { BuiltinMarkdown } from "../markdown"
import type { NativeProps } from "../node-view"
import { mergeStyle } from "../node-view"
import { num, str } from "./shared"

export function TextNative({ props, rootProps }: NativeProps) {
  const lines = num(props.lines, 0)
  const align = props.align === `end` ? `end` : props.align === `center` ? `center` : props.align === `start` ? `start` : undefined
  const style: Record<string, string | number> = {}
  if (align) style.textAlign = align
  if (lines > 1) style.WebkitLineClamp = lines
  const live = props.live === `polite` || props.live === `assertive` ? props.live : undefined
  return (
    <span {...(rootProps as Record<string, unknown>)} aria-live={live} aria-atomic={live ? true : undefined} data-lines={lines > 0 ? lines : undefined} data-clamp={lines > 1 ? `` : undefined} style={mergeStyle(rootProps, Object.keys(style).length ? style : undefined)}>
      {str(props.text)}
    </span>
  )
}

export function MarkdownNative({ props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const text = str(props.text)
  const Host = ctx.host.Markdown
  const lines = num(props.lines, 0)
  return (
    <div {...(rootProps as Record<string, unknown>)} data-lines={lines > 0 ? lines : undefined} style={mergeStyle(rootProps, lines > 0 ? ({ WebkitLineClamp: lines } as React.CSSProperties) : undefined)}>
      {Host ? <Host text={text} className="xui-md" /> : <BuiltinMarkdown text={text} link={ctx.host.resolveUrl} />}
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
