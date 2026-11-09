// #EDE-65 lane M (React's natives/embed.tsx, fixtures/embed.json): Embed, a
// nested surface painted in place, SANDBOXED. The resolved `surface` (a
// DATA prop: only a top-level binding read the host) mounts through
// `embedMount` (a bad one = the error Alert) as its own
// `ExponentialSurface`: its own data model (an inner `set` writes it, never
// the host's), its own surface id (`embedSurfaceId(host, painted id)`), the
// host's theme, mode, strings, locale, direction and presentation hooks
// (`embedHost`). Every inner event fires the Embed's `action` with
// `embedPayload`; an inner host function is denied (its action stops). A
// deep change of the surface remounts the card fresh.

import { Show, createMemo, type Component, type JSX } from "solid-js"
import { CORE_CATALOG_ID, EMBED_EVENT, embedMount, embedPayload, embedSurfaceId, reduceNested } from "@exponential_app/eui"
import { embedBoxStyle, embedHost } from "@exponential_app/eui/web"
import { InstanceContext, ScopeContext, useSurfaceContext } from "../context"
import type { HostPlugin } from "../host"
import { mergeStyle, type NativeProps } from "../node-view"
import { ExponentialSurface, sameData } from "../surface"

const ROOT = () => ``

export function EmbedNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  // A new card (a fresh mount) only when the surface's CONTENT changed.
  const surface = createMemo(() => props.props.surface, undefined, { equals: sameData })
  const mounted = createMemo(() => {
    const { card } = embedMount(surface())
    const { root, templates } = reduceNested(card.root, { catalogId: CORE_CATALOG_ID })
    return { root, templates, data: card.data }
  })
  const host = createMemo(() => embedHost(ctx.host, (e) => props.emit(EMBED_EVENT, embedPayload(e))) as HostPlugin)
  return (
    <div {...props.rootProps} style={mergeStyle(props.rootProps, embedBoxStyle(props.node.style as Record<string, unknown> | undefined))}>
      {/* The nested surface starts at ITS root: no host template scope or instance suffix. */}
      <ScopeContext.Provider value={ROOT}>
        <InstanceContext.Provider value={ROOT}>
          <Show when={mounted()} keyed>
            {(m) => (
              <ExponentialSurface
                id={embedSurfaceId(ctx.surfaceId, props.domId)}
                root={m.root}
                templates={m.templates}
                data={m.data}
                theme={ctx.theme}
                mode={ctx.mode}
                contrast="normal"
                locale={ctx.locale}
                strings={ctx.strings as Record<string, string>}
                direction={ctx.direction}
                states={ctx.states}
                host={host()}
              />
            )}
          </Show>
        </InstanceContext.Provider>
      </ScopeContext.Provider>
    </div>
  )
}

export const NATIVES_EMBED: Record<string, Component<NativeProps>> = {
  Embed: EmbedNative,
}
