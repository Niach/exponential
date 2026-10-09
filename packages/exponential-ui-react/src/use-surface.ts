// VAPP-87: `useSurface` — the TS reducer as React state. Feed it A2UI v0.9
// server messages (`createSurface`, `updateComponents`, `updateDataModel`,
// `deleteSurface`) or a nested authoring tree, read back the normalized
// root, the issues and the data model, and hand the result to
// `<ExponentialSurface surface={…}>`.

import { useCallback, useMemo, useRef, useState } from "react"
import { CORE_CATALOG_ID, reduceNested, reduceSurface } from "@exponential-at/ui"
import type { ExtensionDef, FlatComponent, NestedNode, ReduceIssue, UiNode } from "@exponential-at/ui"
import { setPointer } from "./data"
import type { DataModel } from "./data"

export interface A2uiMessage {
  version?: string
  createSurface?: { surfaceId: string; catalogId: string; theme?: unknown; sendDataModel?: boolean }
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
  data: DataModel
  /** True after `deleteSurface`. */
  deleted: boolean
  /** The flat components (for template children). */
  components: readonly FlatComponent[]
  extensions: readonly ExtensionDef[]
  apply: (message: A2uiMessage) => void
  setData: (pointer: string, value: unknown) => void
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
}

export function useSurface(options: UseSurfaceOptions = {}): SurfaceState {
  const surfaceId = options.surfaceId ?? `main`
  const extensions = options.extensions ?? []
  const initialRef = useRef<Inner>({
    catalogId: options.catalogId ?? CORE_CATALOG_ID,
    components: [],
    nested: options.initial ?? null,
    data: options.data ?? {},
    deleted: false,
  })
  const [inner, setInner] = useState<Inner>(initialRef.current)

  const apply = useCallback(
    (message: A2uiMessage) => {
      setInner((cur) => {
        if (message.createSurface) {
          if (message.createSurface.surfaceId !== surfaceId) return cur
          return { ...cur, catalogId: message.createSurface.catalogId, components: [], nested: null, deleted: false }
        }
        if (message.updateComponents) {
          if (message.updateComponents.surfaceId !== surfaceId) return cur
          // A later update replaces components by id and keeps the rest.
          const byId = new Map(cur.components.map((c) => [c.id, c]))
          for (const c of message.updateComponents.components) byId.set(c.id, c)
          return { ...cur, components: [...byId.values()], nested: null, deleted: false }
        }
        if (message.updateDataModel) {
          if (message.updateDataModel.surfaceId !== surfaceId) return cur
          const { path, value } = message.updateDataModel
          return { ...cur, data: setPointer(cur.data, path ?? ``, value) }
        }
        if (message.deleteSurface) {
          if (message.deleteSurface.surfaceId !== surfaceId) return cur
          return { ...cur, components: [], nested: null, deleted: true }
        }
        return cur
      })
    },
    [surfaceId]
  )

  const setData = useCallback((pointer: string, value: unknown) => {
    setInner((cur) => ({ ...cur, data: setPointer(cur.data, pointer, value) }))
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
    setNested,
    reset,
  }
}
