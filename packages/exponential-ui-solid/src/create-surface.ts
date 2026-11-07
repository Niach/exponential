// FEED-128: `createSurface` — the TS reducer as a Solid store (React's
// `useSurface`). Feed it A2UI 1.0 agent messages, 0.9 through the core's bridge (`createSurface`,
// `updateComponents`, `updateDataModel`, `deleteSurface`) or a nested
// authoring tree, read back the normalized root, the issues and the data
// model (live getters), and hand the result to
// `<ExponentialSurface surface={…}>`.

import { batch, createMemo, createSignal } from "solid-js"
import { CORE_CATALOG_ID, newSurfaceBridge, reduceNested, reduceSurface, surfaceUpdates } from "@exponential_app/eui"
import type { ExtensionDef, FlatComponent, NestedNode, ReduceIssue, ResolvedTheme, UiNode } from "@exponential_app/eui"
import { setPointer } from "@exponential_app/eui/web"
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
  const [writeEpoch, setWriteEpoch] = createSignal(0)

  const bridge = newSurfaceBridge(options.catalogId ?? CORE_CATALOG_ID)
  const apply = (message: A2uiMessage) => {
    // Round 9 §1.5: the core parses the message (version, bridge, entries).
    const { updates } = surfaceUpdates(message, bridge, { extensions: extensions as ExtensionDef[] })
    batch(() => {
      for (const update of updates) {
        if (update.surfaceId !== surfaceId) continue
        if (update.kind === `create`) {
          const { catalogId, components, dataModel } = update
          setInner((cur) => ({ ...cur, catalogId: catalogId ?? cur.catalogId, components, nested: null, deleted: false }))
          if (dataModel) setDataModel(dataModel)
        } else if (update.kind === `components`) {
          // A later update replaces components by id and keeps the rest.
          setInner((cur) => {
            const byId = new Map(cur.components.map((c) => [c.id, c]))
            for (const c of update.components) byId.set(c.id, c)
            return { ...cur, components: [...byId.values()], nested: null, deleted: false }
          })
        } else if (update.kind === `data`) setDataModel((cur) => setPointer(cur, update.path, update.value))
        else setInner((cur) => ({ ...cur, components: [], nested: null, deleted: true }))
      }
    })
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
    get writeEpoch() {
      return writeEpoch()
    },
    get components() {
      return inner().components
    },
    extensions,
    apply,
    setData: (pointer, value) => setDataModel((cur) => setPointer(cur, pointer, value)),
    assign: (pointer, value) =>
      batch(() => {
        setDataModel((cur) => setPointer(cur, pointer, value))
        setWriteEpoch((n) => n + 1)
      }),
    setNested: (tree) => setInner((cur) => ({ ...cur, nested: tree, components: [], deleted: false })),
    reset: () => {
      setInner(initial)
      setDataModel(initialData)
    },
  }
}
