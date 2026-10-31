// VAPP-110: Exponential apps, the pure reference every client mirrors
// (fixtures/apps.json is the spec; desktop `domain::apps`, iOS
// `Apps.swift`, Android `domain/Apps.kt` replay its cases).

import fixture from "../fixtures/apps.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }

export const APPS_COPY = fixture.copy
export const APPS_PANES = fixture.panes
export const APPS_PREVIEW = fixture.preview
export const APPS_TEST_IDS = fixture.testIds
export const APPS_EDITOR_TABS = fixture.editorTabs

/** VAPP-177: the editor's tabs (web ≥ md and desktop: Preview | Versions |
 *  Settings; phones: Chat | Preview + the `…` menu's sheets). */
export type AppEditorTab = `chat` | `preview` | `data` | `versions` | `settings`
/** The phone menu items that open a tab body as a sheet. */
export type AppEditorSheet = `versions` | `settings`

/** A requested tab (web `?tab=`, the desktop settings nav) → the tab on
 *  screen and, on phones, the sheet over it (`editorTabCases`). */
export function editorTab(requested: string | null | undefined, phone: boolean): { tab: AppEditorTab; sheet: AppEditorSheet | null } {
  if (phone) {
    if (requested === `versions` || requested === `settings`) return { tab: APPS_EDITOR_TABS.phoneDefault as AppEditorTab, sheet: requested }
    const tab = APPS_EDITOR_TABS.phone.includes(requested ?? ``) ? requested! : APPS_EDITOR_TABS.phoneDefault
    return { tab: tab as AppEditorTab, sheet: null }
  }
  const tab = APPS_EDITOR_TABS.wide.includes(requested ?? ``) ? requested! : APPS_EDITOR_TABS.default
  return { tab: tab as AppEditorTab, sheet: null }
}

/** VAPP-176: a DRAFT = an app with no version yet (New app's "Untitled app"
 *  until the editor session publishes). */
export function isDraftApp(app: { headVersion: number }): boolean {
  return app.headVersion === 0
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
    ...(isDraftApp(app)
      ? [`It is a new DRAFT with a placeholder name: on your FIRST exponential_apps {op: "upsert"} also pass name, icon, color and description that fit what the person asks for. The slug follows the new name; use the slug the upsert returns from then on.`]
      : []),
    `An app is a declarative Exponential UI package (manifest + A2UI templates + bindings + a function allowlist; data, never code). Read the current package with exponential_apps {op: "get", slug: "${app.slug}"}; learn the components, bindings, host functions and records with exponential_apps {op: "catalog"}; publish every change as a new version with exponential_apps {op: "upsert", slug: "${app.slug}", package, note}. An app keeps its own data as RECORDS (declare the kinds in the manifest, bind exp:records, write with harness.records.upsert); seed or inspect them with exponential_apps {op: "records_upsert" | "records_list", slug: "${app.slug}", kind, data}. The person sees the live preview beside this chat, so keep replies to one or two lines and publish early.`,
  ]
  return lines.join(`\n`)
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
