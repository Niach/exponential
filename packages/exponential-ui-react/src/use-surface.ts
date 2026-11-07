// VAPP-87: `useSurface` — the TS reducer as React state. Feed it A2UI 1.0
// agent messages (0.9 through the core's bridge, round 9 §1.5) (`createSurface`, `updateComponents`, `updateDataModel`,
// `deleteSurface`) or a nested authoring tree, read back the normalized
// root, the issues and the data model, and hand the result to
// `<ExponentialSurface surface={…}>`.

import { useCallback, useRef, useSyncExternalStore } from "react"
import { CORE_CATALOG_ID, SurfaceStore } from "@exponential_app/eui"
import type { ExtensionDef, FlatComponent, NestedNode, ReduceIssue, ResolvedTheme, UiNode } from "@exponential_app/eui"
import type { DataModel } from "@exponential_app/eui/web"

export interface A2uiMessage {
  version?: string
  createSurface?: { surfaceId: string; catalogId?: string; components?: FlatComponent[]; dataModel?: Record<string, unknown>; sendDataModel?: boolean; metadata?: { extensions?: Record<string, unknown> } }
  updateComponents?: { surfaceId: string; components: FlatComponent[] }
  updateDataModel?: { surfaceId: string; path?: string; value?: unknown }
  deleteSurface?: { surfaceId: string }
}

export interface UseSurfaceOptions {
  /** The surface id messages must carry; default `main`. */
  surfaceId?: string
  catalogId?: string
  extensions?: readonly ExtensionDef[]
  /** A nested tree to start from (fixtures, static screens). */
  initial?: NestedNode
  /** The initial data model. */
  data?: DataModel
}

export interface SurfaceState {
  surfaceId: string
  catalogId: string
  root: UiNode | null
  issues: ReduceIssue[]
  /** Round 2: the nodes a data `template` renders per item, by component id
   *  (`ReduceResult.templates`: lifted out of the tree, never painted in
   *  place). */
  templates?: Readonly<Record<string, UiNode>>
  /** The server's `createSurface.theme`, resolved (a host surface). */
  theme?: ResolvedTheme
  data: DataModel
  /** True after `deleteSurface`. */
  deleted: boolean
  /** The flat components (for template children). */
  components: readonly FlatComponent[]
  extensions: readonly ExtensionDef[]
  apply: (message: A2uiMessage) => void
  setData: (pointer: string, value: unknown) => void
  /** #VAPP-288: the count of PROGRAMMATIC writes (`assign`); absent = 0. */
  writeEpoch?: number
  /** #VAPP-288: a PROGRAMMATIC write (an action's `set` step): `setData`
   *  plus a `writeEpoch` bump, so a focused field bound to `pointer` adopts
   *  it. Absent = the surface counts its own (`setData` + its epoch). */
  assign?: (pointer: string, value: unknown) => void
  /** Replace the tree with a nested authoring tree. */
  setNested: (tree: NestedNode) => void
  reset: () => void
}

/** One empty list: a fresh `[]` per render would re-reduce on every render. */
const NO_EXTENSIONS: readonly ExtensionDef[] = []

export function useSurface(options: UseSurfaceOptions = {}): SurfaceState {
  const surfaceId = options.surfaceId ?? `main`
  const extensions = options.extensions ?? NO_EXTENSIONS
  const catalogId = options.catalogId ?? CORE_CATALOG_ID
  // The latest extensions, read when the store reduces.
  const latest = useRef(extensions)
  const made = useRef<{ store: SurfaceStore; start: { catalogId: string; nested: NestedNode | null; data: DataModel } } | null>(null)
  if (made.current?.store.surfaceId !== surfaceId) {
    const start = made.current?.start ?? { catalogId, nested: options.initial ?? null, data: options.data ?? {} }
    made.current = { start, store: new SurfaceStore(surfaceId, start.catalogId, () => latest.current, { nested: start.nested ?? undefined, data: start.data }) }
  }
  const { store, start } = made.current
  if (latest.current !== extensions) {
    latest.current = extensions
    store.stale()
  }
  useSyncExternalStore(
    useCallback((notify: () => void) => store.subscribe(notify), [store]),
    () => store.version
  )

  // Round 9 §1.5: `SurfaceStore.apply` is the one entry (version, bridge, entries).
  const apply = useCallback((message: A2uiMessage) => void store.apply(message), [store])
  const setData = useCallback((pointer: string, value: unknown) => void store.setData(pointer, value), [store])
  const assign = useCallback((pointer: string, value: unknown) => void store.assign(pointer, value), [store])
  const setNested = useCallback((tree: NestedNode) => store.setNested(tree), [store])
  const reset = useCallback(() => store.restore(start.catalogId, start.nested, start.data), [store, start])

  return {
    surfaceId,
    catalogId: store.catalogId ?? start.catalogId,
    root: store.root,
    issues: store.issues,
    templates: store.templates,
    data: store.data,
    deleted: store.deleted,
    components: store.components,
    extensions,
    apply,
    setData,
    writeEpoch: store.writeEpoch,
    assign,
    setNested,
    reset,
  }
}
