// FEED-128: the Solid side of the host API (React's host-surface.tsx). An
// `ExponentialHost` (transport, router, surfaces, sources, functions,
// policy) feeds `<HostSurface>`; the renderer's interactions go back
// through `hostPlugin(host)`. Nothing here knows a transport or a backend.

import { Show, createEffect, createMemo, createSignal, onCleanup, splitProps, type Accessor, type JSX } from "solid-js"
import type { ExponentialHost, SurfaceStore, TransportStatus } from "@exponential-at/ui"
import type { HostPlugin } from "./host"
import type { SurfaceState } from "./create-surface"
import { ExponentialSurface, type ExponentialSurfaceProps } from "./surface"
import { latest } from "./node-view"

/** The renderer callbacks that route through the host (React's
 *  `hostPlugin`, identical). */
export function hostPlugin(host: ExponentialHost, base: HostPlugin = {}): HostPlugin {
  return {
    ...base,
    onAction: (e) => {
      host.action({ surfaceId: e.surfaceId, componentId: e.componentId, name: e.name, context: e.context, payload: e.payload, timestamp: e.timestamp })
      return base.onAction?.(e)
    },
    onFunctionCall: async (call) => {
      const outcome = await host.callFunction(call)
      await base.onFunctionCall?.(call)
      return outcome
    },
    openUrl: (url) => {
      host.openUrl(url)
    },
    urls: host.urlPolicy(),
    media: host.mediaOptions(),
    mediaRequest: base.mediaRequest ?? ((src) => host.mediaRequest(src)),
    onPaintError: (error) => {
      host.paintError(error)
      base.onPaintError?.(error)
    },
  }
}

/** A signal bumped on every host change (unsubscribed on cleanup). */
function hostTick(host: ExponentialHost): Accessor<number> {
  const [tick, setTick] = createSignal(0)
  onCleanup(host.subscribe(() => setTick((n) => n + 1)))
  return tick
}

/** The ids of the host's surfaces, live (React's `useHostSurfaceIds`). */
export function createHostSurfaceIds(host: ExponentialHost): Accessor<string[]> {
  const tick = hostTick(host)
  return createMemo(
    () => {
      tick()
      return host.surfaceIds()
    },
    undefined,
    { equals: (a, b) => a.join(`\n`) === b.join(`\n`) }
  )
}

/** The host's transport state and the catalog it could not render, as live
 *  getters (React's `useHostStatus`). */
export function createHostStatus(host: ExponentialHost): { readonly status: TransportStatus; readonly detail?: string; readonly unsupportedCatalog?: string } {
  const tick = hostTick(host)
  return {
    get status() {
      tick()
      return host.status
    },
    get detail() {
      tick()
      return host.statusDetail
    },
    get unsupportedCatalog() {
      tick()
      return host.unsupportedCatalog
    },
  }
}

/** React's name. */
export const useHostStatus = createHostStatus

/** One host surface as the `SurfaceState` `<ExponentialSurface>` paints
 *  (React's `useHostSurface`); null while the host has none. */
export function createHostSurface(host: ExponentialHost, surfaceId: string): Accessor<SurfaceState | null> {
  const tick = hostTick(host)
  const store = createMemo(() => {
    tick()
    return host.surface(surfaceId) ?? null
  })
  const [version, setVersion] = createSignal(0)
  createEffect(() => {
    const s = store()
    if (!s) return
    onCleanup(s.subscribe(() => setVersion((n) => n + 1)))
  })
  return createMemo(() => {
    version()
    const s = store()
    return s ? stateOf(host, s) : null
  })
}

function stateOf(host: ExponentialHost, store: SurfaceStore): SurfaceState {
  return {
    surfaceId: store.surfaceId,
    catalogId: store.catalogId,
    root: store.root,
    issues: store.issues,
    templates: store.templates,
    theme: store.theme,
    data: store.data,
    deleted: false,
    components: store.components,
    extensions: host.extensionDefs,
    apply: (message) => {
      host.receive(message)
    },
    setData: (pointer, value) => store.setData(pointer, value),
    setNested: () => {
      throw new Error(`exponential-ui: a host surface is fed by its transport, not setNested`)
    },
    reset: () => {},
  }
}

export interface HostSurfaceProps extends Omit<ExponentialSurfaceProps, `surface` | `root` | `data` | `host`> {
  host: ExponentialHost
  surfaceId: string
  /** Icons, markdown, input observers… (actions, functions, urls and media
   *  route through the host). */
  plugin?: HostPlugin
  /** Shown while the host has no surface with this id. */
  fallback?: JSX.Element
}

/** A surface the host's transport feeds. */
export function HostSurface(props: HostSurfaceProps): JSX.Element {
  const [local, rest] = splitProps(props, [`host`, `surfaceId`, `plugin`, `fallback`])
  const surface = createHostSurface(local.host, local.surfaceId)
  const plugin = createMemo(() => hostPlugin(local.host, local.plugin))
  const last = latest(surface)
  return (
    <Show when={surface()} fallback={local.fallback}>
      <ExponentialSurface {...rest} theme={last().theme ?? rest.theme} id={rest.id ?? local.surfaceId} surface={last()} host={plugin()} />
    </Show>
  )
}
