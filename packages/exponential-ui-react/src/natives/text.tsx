// VAPP-87: Text, Markdown, Icon.

import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import { BuiltinMarkdown } from "../markdown"
import type { NativeProps } from "../node-view"
import { num, str, useParts } from "./shared"

export function TextNative({ props, rootProps }: NativeProps) {
  const lines = num(props.lines, 0)
  const align = props.align === `end` ? `end` : props.align === `center` ? `center` : props.align === `start` ? `start` : undefined
  const style: Record<string, string | number> = {}
  if (align) style.textAlign = align
  if (lines > 1) style.WebkitLineClamp = lines
  return (
    <span {...(rootProps as Record<string, unknown>)} data-lines={lines > 0 ? lines : undefined} data-clamp={lines > 1 ? `` : undefined} style={Object.keys(style).length ? style : undefined}>
      {str(props.text)}
    </span>
  )
}

export function MarkdownNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const text = str(props.text)
  const Host = ctx.host.Markdown
  const part = useParts(node, props)
  void part
  return (
    <div {...(rootProps as Record<string, unknown>)}>
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
