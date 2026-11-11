// FEED-128 (React's natives/text.tsx): Text (`live` → an aria-live region;
// EXP-1324 `shimmer` → `data-xui-shimmer`), Markdown (`lines` truncation),
// Icon.

import { createEffect, createSignal, For, onCleanup, Show, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { displayString } from "@exponential_app/eui"
import { liveOf, propAttribute, textLinkSegments, watchMiddleTruncate } from "@exponential_app/eui/web"
import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import { BuiltinMarkdown } from "../markdown"
import { mergeStyle, type NativeProps } from "../node-view"
import { num, str } from "./shared"

/** #EXP-1674: a host's text links: the text's own colour, underlined; a tap
 *  opens through the URL policy and never presses the Row behind it. */
const TEXT_LINK_STYLE = { color: `inherit`, "text-decoration": `underline` } as const

export function TextNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const segments = () => (props.props.hostLinks === true ? textLinkSegments(ctx.host, displayString(props.props.text)) : null)
  // #EXP-1778 (React's): `truncate` keeps one line; `middle` shows start…end.
  const truncate = () => (props.props.truncate === `middle` ? `middle` : props.props.truncate === `end` ? `end` : undefined)
  const lines = () => (truncate() ? 1 : num(props.props.lines, 0))
  const full = () => displayString(props.props.text)
  const [el, setEl] = createSignal<HTMLSpanElement>()
  const [middle, setMiddle] = createSignal<string | null>(null)
  createEffect(() => {
    const node = el()
    const text = truncate() === `middle` && !segments() ? full() : null
    if (text === null || !node) return setMiddle(null)
    onCleanup(watchMiddleTruncate(node, text, setMiddle))
  })
  const align = () => (props.props.align === `end` ? `end` : props.props.align === `center` ? `center` : props.props.align === `start` ? `start` : undefined)
  // `align` is a base-layer rule (`data-xui-align`), so a node style
  // `textAlign` (the node layer) wins over it.
  // Round 9 §5.1: `accessibility.live` (already on the root) wins over `live`.
  const live = () => liveOf(props.rootProps, props.props.live)
  return (
    <span
      {...props.rootProps}
      ref={setEl}
      data-truncate={truncate()}
      aria-live={live()}
      aria-atomic={live() && live() !== `off` ? true : undefined}
      data-xui-align={align()}
      data-lines={lines() > 0 ? lines() : undefined}
      data-clamp={lines() > 1 ? `` : undefined}
      data-xui-shimmer={props.props.shimmer === true ? `` : undefined}
      style={mergeStyle(props.rootProps, lines() > 1 ? { WebkitLineClamp: lines() } : undefined)}
    >
      <Show when={segments()} fallback={middle() ?? full()}>
        {(shown) => (
          <For each={shown()}>
            {(segment) =>
              segment.href ? (
                <a
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
            }
          </For>
        )}
      </Show>
    </span>
  )
}

export function MarkdownNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const text = () => displayString(props.props.text)
  const lines = () => num(props.props.lines, 0)
  // Round 9 §9.1: the inline profile is always the built-in painter.
  const inline = () => (props.props.profile === `inline` ? (props.props.variant === `caption` ? (`caption` as const) : (`body` as const)) : undefined)
  return (
    <div {...props.rootProps} {...(ctx.host.markdownOptions?.density ? { [propAttribute(false, `density`)]: ctx.host.markdownOptions.density } : {})} data-lines={lines() > 0 ? lines() : undefined} style={mergeStyle(props.rootProps, lines() > 0 ? { WebkitLineClamp: lines() } : undefined)}>
      {ctx.host.Markdown && !inline() ? <Dynamic component={ctx.host.Markdown} text={text()} class="xui-md" /> : <BuiltinMarkdown text={text()} host={ctx.host} id={props.domId} inline={inline()} />}
    </div>
  )
}

export function IconNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const name = () => str(props.props.name, `ui-icon-placeholder`)
  const label = () => str(props.props.label)
  return (
    <span {...props.rootProps} role={label() ? `img` : undefined} aria-label={label() || undefined} aria-hidden={label() ? undefined : true}>
      <IconGlyph icons={ctx.host.icons} name={name()} />
    </span>
  )
}
