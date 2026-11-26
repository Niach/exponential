// VAPP-289 (round 9 §1.5): the BARE surface path. A renderer without a host
// (React `useSurface`, Solid `createSurface`, `SurfaceStore.apply`) feeds
// messages straight into one surface; this is the one place that parses them:
// the version check, then the entry check per component (§5.2). The Rust core's `Surface::apply` mirrors it.

import { checkEntry, checkEnvelope, entryRefusal } from "./entry"
import type { ExtensionDef, FlatComponent } from "./types"

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

/** The stream state of one bare surface (its catalog). */
export interface SurfaceStream {
  catalogId?: string
}

export interface SurfaceUpdatesOptions {
  extensions?: readonly ExtensionDef[]
}

/** One message → what it does to a surface. Never throws: `refused` lists
 *  what was dropped. Kinds a bare surface has no use for are ignored. */
export function surfaceUpdates(message: unknown, stream: SurfaceStream, options: SurfaceUpdatesOptions = {}): { updates: SurfaceUpdate[]; refused: SurfaceRefusal[] } {
  const updates: SurfaceUpdate[] = []
  const refused: SurfaceRefusal[] = []
  if (!isObject(message)) return { updates, refused: [{ message: `a message is a JSON object` }] }
  const envelope = checkEnvelope(message)
  if (envelope.kind === `refused`) return { updates, refused: [{ message: envelope.reason }] }
  const entries = (kind: string, list: unknown): FlatComponent[] => {
    const kept: FlatComponent[] = []
    if (!Array.isArray(list)) return kept
    list.forEach((entry, i) => {
      const failure = checkEntry(entry, { catalogId: stream.catalogId, extensions: options.extensions })
      if (failure) refused.push(entryRefusal(kind, i, failure))
      else kept.push(entry as FlatComponent)
    })
    return kept
  }
  const create = message.createSurface
  if (isObject(create) && typeof create.surfaceId === `string`) {
    if (typeof create.catalogId === `string`) stream.catalogId = create.catalogId
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
