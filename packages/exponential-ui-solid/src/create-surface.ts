// FEED-128: `createSurface` — the TS reducer as a Solid store (React's
// `useSurface`). Feed it A2UI 1.0 agent messages, 0.9 through the core's bridge (`createSurface`,
// `updateComponents`, `updateDataModel`, `deleteSurface`) or a nested
// authoring tree, read back the normalized root, the issues and the data
// model (live getters), and hand the result to
// `<ExponentialSurface surface={…}>`.

import { batch, createSignal } from "solid-js"
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
  /** #VAPP-288: the count of PROGRAMMATIC writes (`assign`); absent = 0. */
  readonly writeEpoch?: number
  /** #VAPP-288: a PROGRAMMATIC write (an action's `set` step): `setData`
   *  plus a `writeEpoch` bump, so a focused field bound to `pointer` adopts
   *  it. Absent = the surface counts its own (`setData` + its epoch). */
  assign?: (pointer: string, value: unknown) => void
  /** Replace the tree with a nested authoring tree. */
  setNested: (tree: NestedNode) => void
  reset: () => void
}

export function createSurface(options: CreateSurfaceOptions = {}): SurfaceState {
  const surfaceId = options.surfaceId ?? `main`
  const extensions = options.extensions ?? []
  const start = { catalogId: options.catalogId ?? CORE_CATALOG_ID, nested: options.initial ?? null, data: (options.data ?? {}) as Record<string, unknown> }
  const store = new SurfaceStore(surfaceId, start.catalogId, () => extensions, { nested: start.nested ?? undefined, data: start.data })
  // The tree and the data are separate signals: a data write never touches
  // what reads the tree (and the store never re-reduces for it).
  const [tree, setTree] = createSignal(store.treeVersion)
  const [data, setDataModel] = createSignal<DataModel>(store.data)
  const [writeEpoch, setWriteEpoch] = createSignal(store.writeEpoch)
  store.subscribe(() =>
    batch(() => {
      setTree(store.treeVersion)
      setDataModel(store.data)
      setWriteEpoch(store.writeEpoch)
    })
  )
  const fromTree = <T>(read: () => T): T => {
    tree()
    return read()
  }

  return {
    surfaceId,
    get catalogId() {
      return fromTree(() => store.catalogId ?? start.catalogId)
    },
    get root() {
      return fromTree(() => store.root)
    },
    get issues() {
      return fromTree(() => store.issues)
    },
    get templates() {
      return fromTree(() => store.templates)
    },
    get data() {
      return data()
    },
    get deleted() {
      return fromTree(() => store.deleted)
    },
    get writeEpoch() {
      return writeEpoch()
    },
    get components() {
      return fromTree(() => store.components)
    },
    extensions,
    // Round 9 §1.5: `SurfaceStore.apply` is the one entry (version, bridge, entries).
    apply: (message) => batch(() => void store.apply(message)),
    setData: (pointer, value) => void store.setData(pointer, value),
    assign: (pointer, value) => void store.assign(pointer, value),
    setNested: (tree) => store.setNested(tree),
    reset: () => store.restore(start.catalogId, start.nested, start.data),
  }
}
