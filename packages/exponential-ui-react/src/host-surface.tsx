// VAPP-91: the React side of the host API. An `ExponentialHost` (from
// `@exponential-at/ui`: transport, router, surfaces, sources, functions,
// policy) feeds `<HostSurface>`; the renderer's interactions go back through
// `hostPlugin(host)`. Nothing here knows a transport or a backend.

import { useCallback, useMemo, useSyncExternalStore } from "react"
import { reduceSurface } from "@exponential-at/ui"
import type { ExponentialHost, SurfaceStore, TransportStatus, UiNode } from "@exponential-at/ui"
import type { HostPlugin } from "./host"
import type { SurfaceState } from "./use-surface"
import { ExponentialSurface, type ExponentialSurfaceProps } from "./surface"

/** The renderer callbacks that route through the host: actions become A2UI
 *  client messages on its transport, host functions pass its policy gate,
 *  urls its URL policy, media its loader. `base` adds the rest (icons,
 *  markdown, onInput…) and may observe actions first. */
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
    mediaRequest: base.mediaRequest ?? ((src) => host.mediaRequest(src)),
  }
}

const noop = () => () => {}

/** The ids of the host's surfaces, re-rendered as surfaces come and go. */
export function useHostSurfaceIds(host: ExponentialHost): string[] {
  const subscribe = useCallback((cb: () => void) => host.subscribe(cb), [host])
  const key = useSyncExternalStore(subscribe, () => host.surfaceIds().join(`\n`))
  return useMemo(() => (key ? key.split(`\n`) : []), [key])
}

/** The host's transport state (`host_offline` when not `open`) and the
 *  catalog it could not render (the catalog-update banner). */
export function useHostStatus(host: ExponentialHost): { status: TransportStatus; detail?: string; unsupportedCatalog?: string } {
  const subscribe = useCallback((cb: () => void) => host.subscribe(cb), [host])
  const status = useSyncExternalStore(subscribe, () => host.status)
  const detail = useSyncExternalStore(subscribe, () => host.statusDetail)
  const unsupportedCatalog = useSyncExternalStore(subscribe, () => host.unsupportedCatalog)
  return { status, detail, unsupportedCatalog }
}

/** One host surface as the `SurfaceState` `<ExponentialSurface>` paints. */
export function useHostSurface(host: ExponentialHost, surfaceId: string): SurfaceState | null {
  const subscribeHost = useCallback((cb: () => void) => host.subscribe(cb), [host])
  const store = useSyncExternalStore(subscribeHost, () => host.surface(surfaceId))
  const subscribeStore = useCallback((cb: () => void) => (store ? store.subscribe(cb) : noop()), [store])
  const version = useSyncExternalStore(subscribeStore, () => store?.version ?? -1)
  return useMemo(() => (store ? stateOf(host, store) : null), [host, store, version])
}

/** Reduced templates per components array (it changes only on
 *  `setComponents`; a `setData` bumps the version, never the array). */
const templateCache = new WeakMap<readonly unknown[], { catalogId: string; defs: unknown; templates: Record<string, UiNode> | undefined }>()

/** The store's lifted templates (round 2 §4). `SurfaceStore` does not
 *  expose its reduce result's `templates` yet, so the components are
 *  reduced again, ONCE per components array, when the tree references a
 *  template. */
function templatesOf(host: ExponentialHost, store: SurfaceStore): Record<string, UiNode> | undefined {
  const own = (store as unknown as { templates?: Record<string, UiNode> }).templates
  if (own) return own
  if (!store.root) return undefined
  const hit = templateCache.get(store.components)
  if (hit && hit.catalogId === store.catalogId && hit.defs === host.extensionDefs) return hit.templates
  const templates = store.components.some((c) => (typeof c.children === `object` && c.children !== null && !Array.isArray(c.children)) || (typeof c.template === `object` && c.template !== null))
    ? reduceSurface(store.components, { catalogId: store.catalogId, extensions: host.extensionDefs }).templates
    : undefined
  templateCache.set(store.components, { catalogId: store.catalogId, defs: host.extensionDefs, templates })
  return templates
}

function stateOf(host: ExponentialHost, store: SurfaceStore): SurfaceState {
  return {
    surfaceId: store.surfaceId,
    catalogId: store.catalogId,
    root: store.root,
    issues: store.issues,
    templates: templatesOf(host, store),
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
  fallback?: React.ReactNode
}

/** A surface the host's transport feeds. */
export function HostSurface({ host, surfaceId, plugin, fallback = null, ...rest }: HostSurfaceProps) {
  const surface = useHostSurface(host, surfaceId)
  const p = useMemo(() => hostPlugin(host, plugin), [host, plugin])
  if (!surface) return <>{fallback}</>
  return <ExponentialSurface {...rest} id={rest.id ?? surfaceId} surface={surface} host={p} />
}
