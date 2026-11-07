// VAPP-87: `useSurface` — the TS reducer as React state. Feed it A2UI 1.0
// agent messages (0.9 through the core's bridge, round 9 §1.5) (`createSurface`, `updateComponents`, `updateDataModel`,
// `deleteSurface`) or a nested authoring tree, read back the normalized
// root, the issues and the data model, and hand the result to
// `<ExponentialSurface surface={…}>`.

import { useCallback, useMemo, useRef, useState } from "react"
import { CORE_CATALOG_ID, newSurfaceBridge, reduceNested, reduceSurface, surfaceUpdates } from "@exponential_app/eui"
import type { ExtensionDef, FlatComponent, NestedNode, ReduceIssue, ResolvedTheme, SurfaceBridge, SurfaceUpdate, UiNode } from "@exponential_app/eui"
import { setPointer } from "@exponential_app/eui/web"
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

interface Inner {
  catalogId: string
  components: FlatComponent[]
  nested: NestedNode | null
  data: DataModel
  deleted: boolean
  writeEpoch: number
}

/** One empty list: a fresh `[]` per render would re-reduce on every render. */
const NO_EXTENSIONS: readonly ExtensionDef[] = []

function applyUpdate(cur: Inner, update: SurfaceUpdate, surfaceId: string): Inner {
  if (update.surfaceId !== surfaceId) return cur
  switch (update.kind) {
    case `create`:
      return { ...cur, catalogId: update.catalogId ?? cur.catalogId, components: update.components, nested: null, deleted: false, ...(update.dataModel ? { data: update.dataModel } : {}) }
    case `components`: {
      // A later update replaces components by id and keeps the rest.
      const byId = new Map(cur.components.map((c) => [c.id, c]))
      for (const c of update.components) byId.set(c.id, c)
      return { ...cur, components: [...byId.values()], nested: null, deleted: false }
    }
    case `data`:
      return { ...cur, data: setPointer(cur.data, update.path, update.value) }
    case `delete`:
      return { ...cur, components: [], nested: null, deleted: true }
  }
}

export function useSurface(options: UseSurfaceOptions = {}): SurfaceState {
  const surfaceId = options.surfaceId ?? `main`
  const extensions = options.extensions ?? NO_EXTENSIONS
  const initialRef = useRef<Inner>({
    catalogId: options.catalogId ?? CORE_CATALOG_ID,
    components: [],
    nested: options.initial ?? null,
    data: options.data ?? {},
    deleted: false,
    writeEpoch: 0,
  })
  const [inner, setInner] = useState<Inner>(initialRef.current)

  const bridge = useRef<SurfaceBridge>(newSurfaceBridge(options.catalogId ?? CORE_CATALOG_ID))
  const apply = useCallback(
    (message: A2uiMessage) => {
      // Round 9 §1.5: the core parses the message (version, bridge, entries).
      const { updates } = surfaceUpdates(message, bridge.current, { extensions })
      setInner((cur) => updates.reduce((next, update) => applyUpdate(next, update, surfaceId), cur))
    },
    [surfaceId, extensions]
  )

  const setData = useCallback((pointer: string, value: unknown) => {
    setInner((cur) => ({ ...cur, data: setPointer(cur.data, pointer, value) }))
  }, [])

  const assign = useCallback((pointer: string, value: unknown) => {
    setInner((cur) => ({ ...cur, data: setPointer(cur.data, pointer, value), writeEpoch: cur.writeEpoch + 1 }))
  }, [])

  const setNested = useCallback((tree: NestedNode) => {
    setInner((cur) => ({ ...cur, nested: tree, components: [], deleted: false }))
  }, [])

  const reset = useCallback(() => setInner(initialRef.current), [])

  const reduced = useMemo((): { root: UiNode | null; issues: ReduceIssue[]; templates?: Record<string, UiNode> } => {
    if (inner.deleted) return { root: null, issues: [] as ReduceIssue[] }
    if (inner.nested) return reduceNested(inner.nested, { catalogId: inner.catalogId, extensions })
    if (inner.components.length === 0) return { root: null, issues: [] as ReduceIssue[] }
    return reduceSurface(inner.components, { catalogId: inner.catalogId, extensions })
    // extensions are a stable array in practice; a new array means a new catalog view.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inner.nested, inner.components, inner.catalogId, inner.deleted, extensions])

  return {
    surfaceId,
    catalogId: inner.catalogId,
    root: reduced.root,
    issues: reduced.issues,
    templates: reduced.templates,
    data: inner.data,
    deleted: inner.deleted,
    components: inner.components,
    extensions,
    apply,
    setData,
    writeEpoch: inner.writeEpoch,
    assign,
    setNested,
    reset,
  }
}
