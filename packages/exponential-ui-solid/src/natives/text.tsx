// FEED-128 (React's natives/text.tsx): Text (`live` → an aria-live region;
// EXP-1324 `shimmer` → `data-xui-shimmer`), Markdown (`lines` truncation),
// Icon.

import { For, Show, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { displayString } from "@exponential_app/eui"
import { textLinkSegments } from "@exponential_app/eui/web"
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
  const lines = () => num(props.props.lines, 0)
  const align = () => (props.props.align === `end` ? `end` : props.props.align === `center` ? `center` : props.props.align === `start` ? `start` : undefined)
  // `align` is a base-layer rule (`data-xui-align`), so a node style
  // `textAlign` (the node layer) wins over it.
  const live = () => (props.props.live === `polite` || props.props.live === `assertive` ? (props.props.live as `polite` | `assertive`) : undefined)
  return (
    <span
      {...props.rootProps}
      aria-live={live()}
      aria-atomic={live() ? true : undefined}
      data-xui-align={align()}
      data-lines={lines() > 0 ? lines() : undefined}
      data-clamp={lines() > 1 ? `` : undefined}
      data-xui-shimmer={props.props.shimmer === true ? `` : undefined}
      style={mergeStyle(props.rootProps, lines() > 1 ? { WebkitLineClamp: lines() } : undefined)}
    >
      <Show when={segments()} fallback={displayString(props.props.text)}>
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
  return (
    <div {...props.rootProps} data-lines={lines() > 0 ? lines() : undefined} style={mergeStyle(props.rootProps, lines() > 0 ? { WebkitLineClamp: lines() } : undefined)}>
      {ctx.host.Markdown ? <Dynamic component={ctx.host.Markdown} text={text()} class="xui-md" /> : <BuiltinMarkdown text={text()} host={ctx.host} />}
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
