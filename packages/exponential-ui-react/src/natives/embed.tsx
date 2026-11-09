// #EDE-65 lane M (fixtures/embed.json): Embed, a nested surface painted in
// place, SANDBOXED. The resolved `surface` (a DATA prop: only a top-level
// binding read the host) mounts through `embedMount` (a bad one = the error
// Alert) as its own `ExponentialSurface`: its own data model (an inner
// `set` writes it, never the host's), its own surface id
// (`embedSurfaceId(host, painted id)`, so two cards' ids never meet), the
// host's theme, mode, strings, locale, direction and presentation hooks
// (`embedHost`). Every inner event fires the Embed's `action` with
// `embedPayload`; an inner host function is denied (its action stops). A
// deep change of the surface remounts the card fresh.

import { useCallback, useMemo, useRef } from "react"
import { CORE_CATALOG_ID, EMBED_EVENT, embedMount, embedPayload, embedSurfaceId, reduceNested } from "@exponential_app/eui"
import { embedBoxStyle, embedHost, type SurfaceActionEvent } from "@exponential_app/eui/web"
import { InstanceContext, ScopeContext, useSurfaceContext } from "../context"
import type { HostPlugin } from "../host"
import { mergeStyle, type NativeProps } from "../node-view"
import { ExponentialSurface, sameData } from "../surface"

export function EmbedNative({ node, props, rootProps, emit, domId }: NativeProps) {
  const ctx = useSurfaceContext()
  // A new generation (a fresh mount) only when the surface's CONTENT changed.
  const last = useRef<{ surface: unknown; generation: number }>({ surface: props.surface, generation: 0 })
  if (last.current.surface !== props.surface && !sameData(last.current.surface, props.surface)) last.current = { surface: props.surface, generation: last.current.generation + 1 }
  const { surface, generation } = last.current
  const mounted = useMemo(() => {
    const { card } = embedMount(surface)
    const { root, templates } = reduceNested(card.root, { catalogId: CORE_CATALOG_ID })
    return { root, templates, data: card.data }
  }, [surface])
  const forward = useCallback((e: SurfaceActionEvent) => emit(EMBED_EVENT, embedPayload(e)), [emit])
  const host = useMemo(() => embedHost(ctx.host, forward) as HostPlugin, [ctx.host, forward])
  return (
    <div {...(rootProps as Record<string, unknown>)} style={mergeStyle(rootProps, embedBoxStyle(node.style as Record<string, unknown> | undefined))}>
      {/* The nested surface starts at ITS root: no host template scope or instance suffix. */}
      <ScopeContext.Provider value="">
        <InstanceContext.Provider value="">
          <ExponentialSurface
            key={generation}
            id={embedSurfaceId(ctx.surfaceId, domId)}
            root={mounted.root}
            templates={mounted.templates}
            data={mounted.data}
            theme={ctx.theme}
            mode={ctx.mode}
            contrast="normal"
            locale={ctx.locale}
            strings={ctx.strings as Record<string, string>}
            direction={ctx.direction}
            states={ctx.states}
            inlineOverlays={ctx.inlineOverlays}
            host={host}
          />
        </InstanceContext.Provider>
      </ScopeContext.Provider>
    </div>
  )
}
