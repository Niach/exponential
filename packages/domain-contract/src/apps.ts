// VAPP-110: Exponential apps, the pure reference every client mirrors
// (fixtures/apps.json is the spec; desktop `domain::apps`, iOS
// `Apps.swift`, Android `domain/Apps.kt` replay its cases).

import fixture from "../fixtures/apps.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }
import { askDockTitle } from "./ask-dock"
import type { DockRow } from "./dock"

export const APPS_COPY = fixture.copy
export const APPS_PANES = fixture.panes
export const APPS_PREVIEW = fixture.preview
export const APPS_TEST_IDS = fixture.testIds
export const APPS_LAYOUT = fixture.layout
export const APPS_DOCK_CARD = fixture.dockCard

/** VAPP-129: the editor's layout on web ≥ md and the IDE. */
export type AppEditorLayout = `floating` | `docked`

/** A stored layout, else the default. */
export function editorLayout(stored: string | null | undefined): AppEditorLayout {
  return stored === `floating` || stored === `docked` ? stored : (APPS_LAYOUT.default as AppEditorLayout)
}

export const APP_SLUG_MAX = contract.apps.slugMax
export const APP_NAME_MAX = contract.apps.nameMax
export const APP_DESCRIPTION_MAX = contract.apps.descriptionMax
export const APP_NOTE_MAX = contract.apps.noteMax
export const APP_PACKAGE_MAX_BYTES = contract.apps.packageMaxBytes
export const APP_TEMPLATE_MAX_COMPONENTS = contract.apps.templateMaxComponents
export const APP_SLUG_PATTERN = new RegExp(contract.apps.slugPattern)
// VAPP-122 records.
export const APP_RECORD_KIND_PATTERN = new RegExp(contract.apps.kindPattern)
export const APP_RECORD_MAX_BYTES = contract.apps.recordMaxBytes
export const APP_RECORD_KINDS_MAX = contract.apps.recordKindsMax

/** `v{n}`, or the "no version" copy while an app has no head. */
export function versionLabel(headVersion: number): string {
  return headVersion > 0 ? APPS_COPY.version.replace(`{n}`, String(headVersion)) : APPS_COPY.noVersion
}

/** A name → its slug: lowercase ASCII letters, digits and single dashes,
 *  diacritics stripped, cut at the cap, `app` when nothing is left; a taken
 *  slug gets `-2`, `-3`, … */
export function slugify(name: string, taken: readonly string[] = []): string {
  const base =
    name
      .normalize(`NFKD`)
      .replace(/[̀-ͯ]/g, ``)
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, `-`)
      .replace(/^-+|-+$/g, ``)
      .slice(0, APP_SLUG_MAX)
      .replace(/-+$/g, ``) || `app`
  if (!taken.includes(base)) return base
  for (let n = 2; ; n++) {
    const suffix = `-${n}`
    const candidate = `${base.slice(0, APP_SLUG_MAX - suffix.length).replace(/-+$/g, ``)}${suffix}`
    if (!taken.includes(candidate)) return candidate
  }
}

export interface AppListLike {
  slug: string
  name: string
}

/** The list order: by name (case-insensitive). No built-in apps. */
export function sortApps<T extends AppListLike>(apps: readonly T[]): T[] {
  return [...apps].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: `base` }))
}

/** The brief an app's EDITOR SESSION gets as the preface of its first
 *  message (every new or continued session): what the app is and the MCP
 *  calls that read and publish it. */
export function editorBrief(app: { name: string; slug: string; headVersion: number; description?: string | null }): string {
  const lines = [
    `You are the editor session of the Exponential app "${app.name}" (slug \`${app.slug}\`, ${versionLabel(app.headVersion)}).${app.description?.trim() ? ` It is for: ${app.description.trim()}` : ``}`,
    `An app is a declarative Exponential UI package (manifest + A2UI templates + bindings + a function allowlist; data, never code). Read the current package with exponential_apps {op: "get", slug: "${app.slug}"}; learn the components, bindings, host functions and records with exponential_apps {op: "catalog"}; publish every change as a new version with exponential_apps {op: "upsert", slug: "${app.slug}", package, note}. An app keeps its own data as RECORDS (declare the kinds in the manifest, bind exp:records, write with harness.records.upsert); seed or inspect them with exponential_apps {op: "records_upsert" | "records_list", slug: "${app.slug}", kind, data}. The person sees the live preview beside this chat, so keep replies to one or two lines and publish early.`,
  ]
  return lines.join(`\n`)
}

/** VAPP-129: the floating editor's ONE DockCard (kind `chat`): the newest
 *  message's first plain line (else the empty chat copy), `Working` while
 *  the editor session is busy. `messages` = the bodies, oldest first. */
export function chatCard(messages: readonly string[], busy: boolean): DockRow {
  const newest = [...messages].reverse().map(askDockTitle).find((line) => line.length > 0)
  return {
    id: APPS_DOCK_CARD.id,
    kind: `chat`,
    tint: `muted`,
    icon: APPS_DOCK_CARD.icon,
    label: APPS_COPY.tabs.chat,
    caption: busy ? APPS_COPY.working : null,
    meta: null,
    title: newest ?? APPS_COPY.chatEmpty,
    expandable: true,
    // The card opens (the chevron); "Move to the side" is the sheet's.
    action: null,
  }
}

/** VAPP-157: the app hosts' media rule (fixture `mediaPolicy`), a plain
 *  string rule so the four hosts agree byte for byte: a `data:` image, audio
 *  or video (never svg), or the instance's own `/api/attachments/<uuid>`
 *  (relative or on `origin` exactly) with nothing after the id. */
export const APP_MEDIA_POLICY = fixture.mediaPolicy
const ATTACHMENT_PATH = new RegExp(`^${APP_MEDIA_POLICY.attachmentPath}[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`, `i`)

export function appMediaAllowed(src: string, origin: string): boolean {
  if (!src || src.trim() !== src) return false
  const lower = src.toLowerCase()
  if (lower.startsWith(`data:`)) {
    const comma = lower.indexOf(`,`)
    if (comma < 0) return false
    const mime = lower.slice(5, comma).split(`;`)[0]!
    return APP_MEDIA_POLICY.dataMimePrefixes.some((p) => mime.startsWith(p) && mime.length > p.length) && !APP_MEDIA_POLICY.deniedDataMimes.includes(mime)
  }
  let path: string
  if (src.startsWith(`/`) && !src.startsWith(`//`)) path = src
  else {
    const base = origin.replace(/\/+$/, ``).toLowerCase()
    if (!base || !lower.startsWith(`${base}/`)) return false
    path = src.slice(base.length)
  }
  return ATTACHMENT_PATH.test(path)
}
