// VAPP-289 (round 9 §1.5): the BARE surface path. A renderer without a host
// (React `useSurface`, Solid `createSurface`, `SurfaceStore.apply`) feeds
// messages straight into one surface; this is the one place that parses them:
// the version check (with the 0.9 bridge), then the entry check per component
// (§5.2). The Rust core's `Surface::apply` mirrors it.

import { ACCEPT_V09, A2UI_VERSION } from "./catalog"
import { checkEntry, checkEnvelope, entryRefusal } from "./entry"
import type { ExtensionDef, FlatComponent } from "./types"
import { emptyUpgrade09State, upgradeMessage09In, type Upgrade09State } from "./upgrade09"

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)

export type SurfaceUpdate =
  | { kind: `create`; surfaceId: string; catalogId?: string; components: FlatComponent[]; dataModel?: Record<string, unknown>; metadata?: { extensions?: Record<string, unknown> } }
  | { kind: `components`; surfaceId: string; components: FlatComponent[] }
  | { kind: `data`; surfaceId: string; path: string; value?: unknown }
  | { kind: `delete`; surfaceId: string }

/** A refused message or entry: INVALID_MESSAGE text, `path` for an entry. */
export interface SurfaceRefusal {
  message: string
  path?: string
}

/** The stream state of one bare surface (the bridge's id maps, its catalog). */
export interface SurfaceBridge {
  upgrade: Upgrade09State
  catalogId?: string
}

export const newSurfaceBridge = (catalogId?: string): SurfaceBridge => ({ upgrade: emptyUpgrade09State(), ...(catalogId === undefined ? {} : { catalogId }) })

export interface SurfaceUpdatesOptions {
  extensions?: readonly ExtensionDef[]
  acceptV09?: boolean
}

/** One message → what it does to a surface. Never throws: `refused` lists
 *  what was dropped. Kinds a bare surface has no use for are ignored. */
export function surfaceUpdates(message: unknown, bridge: SurfaceBridge, options: SurfaceUpdatesOptions = {}): { updates: SurfaceUpdate[]; refused: SurfaceRefusal[] } {
  const updates: SurfaceUpdate[] = []
  const refused: SurfaceRefusal[] = []
  if (!isObject(message)) return { updates, refused: [{ message: `a message is a JSON object` }] }
  const envelope = checkEnvelope(message, options.acceptV09 ?? ACCEPT_V09)
  if (envelope.kind === `refused`) return { updates, refused: [{ message: envelope.reason }] }
  if (envelope.kind === `upgrade`) {
    const messages = upgradeMessage09In(message, bridge.upgrade)
    for (const m of messages) {
      if (!isObject(m) || m.version !== A2UI_VERSION) refused.push({ message: `upgrade09 produced no 1.0 message` })
      else {
        const out = surfaceUpdates(m, bridge, { ...options, acceptV09: false })
        updates.push(...out.updates)
        refused.push(...out.refused)
      }
    }
    return { updates, refused }
  }
  const entries = (kind: string, list: unknown): FlatComponent[] => {
    const kept: FlatComponent[] = []
    if (!Array.isArray(list)) return kept
    list.forEach((entry, i) => {
      const failure = checkEntry(entry, { catalogId: bridge.catalogId, extensions: options.extensions })
      if (failure) refused.push(entryRefusal(kind, i, failure))
      else kept.push(entry as FlatComponent)
    })
    return kept
  }
  const create = message.createSurface
  if (isObject(create) && typeof create.surfaceId === `string`) {
    if (typeof create.catalogId === `string`) bridge.catalogId = create.catalogId
    updates.push({
      kind: `create`,
      surfaceId: create.surfaceId,
      ...(typeof create.catalogId === `string` ? { catalogId: create.catalogId } : {}),
      components: entries(`createSurface`, create.components),
      ...(isObject(create.dataModel) ? { dataModel: create.dataModel } : {}),
      ...(isObject(create.metadata) ? { metadata: create.metadata } : {}),
    })
  }
  const components = message.updateComponents
  if (isObject(components) && typeof components.surfaceId === `string`) updates.push({ kind: `components`, surfaceId: components.surfaceId, components: entries(`updateComponents`, components.components) })
  const data = message.updateDataModel
  if (isObject(data) && typeof data.surfaceId === `string`) {
    const path = data.path === undefined || data.path === `/` ? `` : String(data.path)
    updates.push(data.value === null || data.value === undefined ? { kind: `data`, surfaceId: data.surfaceId, path } : { kind: `data`, surfaceId: data.surfaceId, path, value: data.value })
  }
  const gone = message.deleteSurface
  if (isObject(gone) && typeof gone.surfaceId === `string`) updates.push({ kind: `delete`, surfaceId: gone.surfaceId })
  return { updates, refused }
}
