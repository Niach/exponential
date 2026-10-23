// FEED-128: `createSurface` — the TS reducer as a Solid store (React's
// `useSurface`). Feed it A2UI v0.9 server messages (`createSurface`,
// `updateComponents`, `updateDataModel`, `deleteSurface`) or a nested
// authoring tree, read back the normalized root, the issues and the data
// model (live getters), and hand the result to
// `<ExponentialSurface surface={…}>`.

import { createMemo, createSignal } from "solid-js"
import { CORE_CATALOG_ID, reduceNested, reduceSurface } from "@exponential_app/ui"
import type { ExtensionDef, FlatComponent, NestedNode, ReduceIssue, ResolvedTheme, UiNode } from "@exponential_app/ui"
import { setPointer } from "@exponential_app/ui/web"
import type { DataModel } from "@exponential_app/ui/web"

export interface A2uiMessage {
  version?: string
  createSurface?: { surfaceId: string; catalogId: string; theme?: unknown; sendDataModel?: boolean }
  updateComponents?: { surfaceId: string; components: FlatComponent[] }
  updateDataModel?: { surfaceId: string; path?: string; value?: unknown }
  deleteSurface?: { surfaceId: string }
}

export interface CreateSurfaceOptions {
  /** The surface id messages must carry; default `main`. */
  surfaceId?: string
  catalogId?: string
  extensions?: readonly ExtensionDef[]
  /** A nested tree to start from (fixtures, static screens). */
  initial?: NestedNode
  /** The initial data model. */
  data?: DataModel
}

/** React's name for the options. */
export type UseSurfaceOptions = CreateSurfaceOptions

/** The surface state (every field a live getter on the object
 *  `createSurface` returns; never destructure it). */
export interface SurfaceState {
  readonly surfaceId: string
  readonly catalogId: string
  readonly root: UiNode | null
  readonly issues: ReduceIssue[]
  /** Round 2: the nodes a data `template` renders per item, by component id. */
  readonly templates?: Readonly<Record<string, UiNode>>
  /** The server's `createSurface.theme`, resolved (a host surface). */
  readonly theme?: ResolvedTheme
  readonly data: DataModel
  /** True after `deleteSurface`. */
  readonly deleted: boolean
  readonly components: readonly FlatComponent[]
  readonly extensions: readonly ExtensionDef[]
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
  deleted: boolean
}

export function createSurface(options: CreateSurfaceOptions = {}): SurfaceState {
  const surfaceId = options.surfaceId ?? `main`
  const extensions = options.extensions ?? []
  const initial: Inner = { catalogId: options.catalogId ?? CORE_CATALOG_ID, components: [], nested: options.initial ?? null, deleted: false }
  const initialData = options.data ?? {}
  // The tree and the data are two signals: a data write never re-reduces.
  const [inner, setInner] = createSignal<Inner>(initial)
  const [data, setDataModel] = createSignal<DataModel>(initialData)

  const apply = (message: A2uiMessage) => {
    if (message.createSurface) {
      if (message.createSurface.surfaceId !== surfaceId) return
      const catalogId = message.createSurface.catalogId
      setInner((cur) => ({ ...cur, catalogId, components: [], nested: null, deleted: false }))
    } else if (message.updateComponents) {
      if (message.updateComponents.surfaceId !== surfaceId) return
      const update = message.updateComponents.components
      // A later update replaces components by id and keeps the rest.
      setInner((cur) => {
        const byId = new Map(cur.components.map((c) => [c.id, c]))
        for (const c of update) byId.set(c.id, c)
        return { ...cur, components: [...byId.values()], nested: null, deleted: false }
      })
    } else if (message.updateDataModel) {
      if (message.updateDataModel.surfaceId !== surfaceId) return
      const { path, value } = message.updateDataModel
      setDataModel((cur) => setPointer(cur, path ?? ``, value))
    } else if (message.deleteSurface) {
      if (message.deleteSurface.surfaceId !== surfaceId) return
      setInner((cur) => ({ ...cur, components: [], nested: null, deleted: true }))
    }
  }

  const reduced = createMemo((): { root: UiNode | null; issues: ReduceIssue[]; templates?: Record<string, UiNode> } => {
    const cur = inner()
    if (cur.deleted) return { root: null, issues: [] }
    if (cur.nested) return reduceNested(cur.nested, { catalogId: cur.catalogId, extensions: extensions as ExtensionDef[] })
    if (cur.components.length === 0) return { root: null, issues: [] }
    return reduceSurface(cur.components, { catalogId: cur.catalogId, extensions: extensions as ExtensionDef[] })
  })

  return {
    surfaceId,
    get catalogId() {
      return inner().catalogId
    },
    get root() {
      return reduced().root
    },
    get issues() {
      return reduced().issues
    },
    get templates() {
      return reduced().templates
    },
    get data() {
      return data()
    },
    get deleted() {
      return inner().deleted
    },
    get components() {
      return inner().components
    },
    extensions,
    apply,
    setData: (pointer, value) => setDataModel((cur) => setPointer(cur, pointer, value)),
    setNested: (tree) => setInner((cur) => ({ ...cur, nested: tree, components: [], deleted: false })),
    reset: () => {
      setInner(initial)
      setDataModel(initialData)
    },
  }
}
