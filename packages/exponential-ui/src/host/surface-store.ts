// One surface's state, the light half of the host: the runtime keeps one per
// surface, and a bare renderer (React `useSurface`, Solid `createSurface`)
// keeps its own and feeds it through `apply` (round 9 §1.5), the one entry.

import { writePointer } from "../dynamic"
import type { Failure } from "../dynamic"
import { surfaceUpdates } from "../surface-updates"
import type { SurfaceRefusal, SurfaceStream } from "../surface-updates"
import { reduceNested, reduceSurface } from "../reducer"
import type { ResolvedTheme } from "../theme-types"
import type { ExtensionDef, FlatComponent, NestedNode, ReduceIssue, UiNode } from "../types"

/** Round 9 §6.5: a routed REACTIVE call: in flight, or settled for the
 *  surface's lifetime. */
export type ReactiveRoute = { state: `pending`; previous: unknown } | { state: `value`; value: unknown } | { state: `failure`; failure: Failure }

export type Listener = () => void

/** One surface's state: the flat components, the data model, the reduced
 *  tree. Mutations notify subscribers; `root` is reduced lazily. */
export class SurfaceStore {
  readonly surfaceId: string
  /** The surface default (round 9 §5.5); undefined = none. */
  catalogId: string | undefined
  packageId?: string
  sendDataModel = false
  /** The flat components, mutated in place while nothing holds them. */
  private list: FlatComponent[] = []
  /** `list` was handed out (`components`): the next update copies first. */
  private shared = false
  private positions = new Map<string, number>()
  data: Record<string, unknown> = {}
  /** The surface's `metadata.extensions.at_exponential_theme`, resolved
   *  (undefined = the renderer's own theme). */
  theme?: ResolvedTheme
  /** Round 9 §5.1: `createSurface.metadata`. */
  metadata?: { extensions?: Record<string, unknown> }
  /** Round 9 §1.3: `<surfaceId>:<n>`. */
  private callCounter = 0
  /** §1.5: the bare path's stream state. */
  private stream: SurfaceStream
  /** A nested authoring tree in place of the flat components. */
  private nested: NestedNode | null
  /** The data model a `createSurface` without `dataModel` starts from. */
  private seed: Record<string, unknown>
  /** True after `deleteSurface`. */
  deleted = false
  /** Round 9 §6.5: the routed reactive calls, by key. */
  routes = new Map<string, ReactiveRoute>()
  /** Round 9 §5.2: what was sent already: (code, path, message). */
  sentIssues = new Set<string>()
  private reduced: { root: UiNode | null; issues: ReduceIssue[]; templates?: Record<string, UiNode> } | null = null
  private listeners = new Set<Listener>()
  version = 0
  /** Bumped when the tree changes (never by a data write): a renderer that
   *  keeps the tree apart from the data keys on it. */
  treeVersion = 0
  /** #VAPP-288: bumped by every PROGRAMMATIC write (`assign`: an action's
   *  `set` step); a focused field adopts a value that came with a move. */
  writeEpoch = 0

  constructor(surfaceId: string, catalogId: string | undefined, private extensions: () => readonly ExtensionDef[], initial: { nested?: NestedNode; data?: Record<string, unknown> } = {}) {
    this.surfaceId = surfaceId
    this.catalogId = catalogId
    this.stream = catalogId === undefined ? {} : { catalogId }
    this.nested = initial.nested ?? null
    this.seed = initial.data ?? {}
    this.data = this.seed
  }

  /** The next `callAgentFunction` id. */
  nextCallId(): string {
    this.callCounter += 1
    return `${this.surfaceId}:${this.callCounter}`
  }

  /** Round 9 §1.5: a message straight into this surface (no router): the
   *  version check, the entry check per component.
   *  Answers what was refused. */
  apply(message: unknown): SurfaceRefusal[] {
    const { updates, refused } = surfaceUpdates(message, this.stream, { extensions: this.extensions() })
    for (const u of updates) {
      if (u.surfaceId !== this.surfaceId) continue
      if (u.kind === `create`) {
        this.catalogId = u.catalogId ?? this.catalogId
        this.list = []
        this.shared = false
        this.positions = new Map()
        this.nested = null
        this.deleted = false
        this.data = u.dataModel ?? this.seed
        this.metadata = u.metadata
        this.routes = new Map()
        this.sentIssues = new Set()
        this.reduced = null
        this.treeVersion += 1
        if (u.components.length) this.setComponents(u.components)
        else this.notify()
      } else if (u.kind === `components`) {
        if (u.components.length) this.setComponents(u.components)
      } else if (u.kind === `data`) {
        const error = this.setData(u.path, u.value)
        if (error) refused.push({ message: error, path: u.path || `/` })
      } else {
        this.list = []
        this.shared = false
        this.positions = new Map()
        this.nested = null
        this.deleted = true
        this.reduced = null
        this.treeVersion += 1
        this.notify()
      }
    }
    return refused
  }

  /** Replace the tree with a nested authoring tree (static screens, fixtures). */
  setNested(tree: NestedNode | null): void {
    this.list = []
    this.shared = false
    this.positions = new Map()
    this.nested = tree
    this.deleted = false
    this.reduced = null
    this.treeVersion += 1
    this.notify()
  }

  /** Back to a starting state: `tree` and `data`, no flat components. */
  restore(catalogId: string | undefined, tree: NestedNode | null, data: Record<string, unknown>): void {
    this.catalogId = catalogId
    this.seed = data
    this.data = data
    this.writeEpoch = 0
    this.setNested(tree)
  }

  /** The flat components as a SNAPSHOT: an array handed out here is never
   *  mutated afterwards (the next update copies first), so a renderer that
   *  keeps it sees a stable value; a stream nobody reads stays linear. */
  get components(): readonly FlatComponent[] {
    this.shared = true
    return this.list
  }

  /** The flat component with this id (undefined = none). */
  component(id: string): FlatComponent | undefined {
    const at = this.positions.get(id)
    return at === undefined ? undefined : this.list[at]
  }

  get root(): UiNode | null {
    return this.reduce().root
  }

  get issues(): ReduceIssue[] {
    return this.reduce().issues
  }

  /** Round 2 §4: the reducer's lifted data templates (absent without any). */
  get templates(): Record<string, UiNode> | undefined {
    return this.reduce().templates
  }

  private reduce() {
    if (!this.reduced) {
      const options = { catalogId: this.catalogId, extensions: this.extensions() }
      this.reduced = this.deleted ? { root: null, issues: [] } : this.nested ? reduceNested(this.nested, options) : this.list.length ? reduceSurface(this.list, options) : { root: null, issues: [] }
    }
    return this.reduced
  }

  /** A2UI: a later update replaces components BY ID and keeps the rest
   *  (an id → position map: a streamed surface costs linear, VAPP-103). */
  setComponents(components: readonly FlatComponent[]): void {
    if (this.shared) {
      this.list = this.list.slice()
      this.shared = false
    }
    for (const c of components) {
      const at = this.positions.get(c.id)
      if (at === undefined) {
        this.positions.set(c.id, this.list.length)
        this.list.push(c)
      } else this.list[at] = c
    }
    this.nested = null
    this.deleted = false
    this.reduced = null
    this.treeVersion += 1
    this.notify()
  }

  /** The tree goes stale without telling anyone (a renderer re-reading in
   *  its own render). */
  stale(): void {
    this.reduced = null
    this.treeVersion += 1
  }

  /** Re-reduce on the next read (the extension set changed). */
  invalidate(): void {
    this.stale()
    this.notify()
  }

  /** Write the data model at `pointer` (`undefined` removes); a refused
   *  write (writePointer) changes nothing and returns the reason. */
  setData(pointer: string, value: unknown): string | undefined {
    const written = writePointer(this.data, pointer, value)
    if (written.error) return written.error
    this.data = written.data as Record<string, unknown>
    this.notify()
    return undefined
  }

  /** #VAPP-288: a PROGRAMMATIC write (an action's `set` step): `setData`
   *  that also bumps `writeEpoch` (one notify), so a focused field bound to
   *  `pointer` adopts it. Host data writes (updateDataModel, a bind, a
   *  record form) go through `setData` and stay under the echo rule. */
  assign(pointer: string, value: unknown): string | undefined {
    const written = writePointer(this.data, pointer, value)
    if (written.error) return written.error
    this.data = written.data as Record<string, unknown>
    this.writeEpoch += 1
    this.notify()
    return undefined
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  notify(): void {
    this.version += 1
    for (const l of [...this.listeners]) l()
  }
}
