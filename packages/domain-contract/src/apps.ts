// VAPP-110: Exponential apps, the pure reference every client mirrors
// (fixtures/apps.json is the spec; desktop `domain::apps`, iOS
// `Apps.swift`, Android `domain/Apps.kt` replay its cases).

import fixture from "../fixtures/apps.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }
import skillsFixture from "../fixtures/skills.json" with { type: "json" }

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

/** eApps V1 (#VAPP-210, fixture `publishCases`): an app's two pointers. */
export interface AppVersions {
  headVersion: number
  publishedVersion: number
}

/** The version the editor's preview shows: an explicitly viewed one, else
 *  nothing (no version), else the HEAD (the draft) while the viewer's own
 *  editor session is live or nothing is published, else the PUBLISHED one. */
export function shownVersion(app: AppVersions, editing: boolean, viewing: number | null): number {
  if (viewing !== null) return viewing
  if (app.headVersion === 0) return 0
  return editing || app.publishedVersion === 0 ? app.headVersion : app.publishedVersion
}

/** Publish is offered while the head is not the published version. */
export function canPublish(app: AppVersions): boolean {
  return app.headVersion > 0 && app.headVersion !== app.publishedVersion
}

/** The list's and the header's version pill: the PUBLISHED version. */
export function listPill(app: AppVersions): string {
  return versionLabel(app.publishedVersion)
}

/** The version a grant is made on and checked against (`apps.update
 *  {headVersion}` carries it): the published one, else the head. */
export function grantVersion(app: AppVersions): number {
  return app.publishedVersion > 0 ? app.publishedVersion : app.headVersion
}

/** A Versions row's trailing: the published badge, the head badge + Publish,
 *  or Preview + Publish. */
export function versionRow(app: AppVersions, number: number): `published` | `head` | `version` {
  if (number === app.publishedVersion) return `published`
  return number === app.headVersion ? `head` : `version`
}

/** eApps R8 (#VAPP-233, fixture `migrationCases`): a version row's sweep columns. */
export interface VersionMigration {
  migrationState: string | null
  migrationTotal: number
  migrationDone: number
  migrationError: string | null
}

/** A Versions row's sweep line (after its subtitle), or null. */
export function versionMigration(row: VersionMigration): string | null {
  if (row.migrationState === `running`) return row.migrationTotal > 0 ? APPS_COPY.migratingOf.replace(`{done}`, String(row.migrationDone)).replace(`{total}`, String(row.migrationTotal)) : APPS_COPY.migrating
  if (row.migrationState === `failed`) return APPS_COPY.migrationFailed.replace(`{error}`, row.migrationError ?? ``)
  return null
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

/** VAPP-213: the built-in skills every app editor session runs with
 *  (fixtures/skills.json `builtin.editorDelivery`), apps first. */
export const EDITOR_SKILLS: readonly string[] = skillsFixture.builtin.editorDelivery

/** The editor WORKFLOW: what an app is and the MCP calls that read, save
 *  and publish it. ONE source: the editor brief below and the built-in
 *  `exponential-apps` skill's SKILL.md (packages/exponential-ui/scripts/
 *  generate-skills.ts) both print it. */
export function editorWorkflow(slug: string): string {
  return `An app is a declarative Exponential UI package (manifest + A2UI templates + bindings + a function allowlist; data, never code). Read the current package with exponential_apps {op: "get", slug: "${slug}"}; learn the components, bindings, host functions and records with exponential_apps {op: "catalog"}; save every change as a new draft version with exponential_apps {op: "upsert", slug: "${slug}", package, note} (members keep running the published version); publish with exponential_apps {op: "publish", slug: "${slug}"} only when the person asks (they also have a Publish button). An app keeps its own data as RECORDS (declare the kinds in the manifest, bind records:<kind>, write with records.upsert); seed or inspect them with exponential_apps {op: "records_upsert" | "records_list", slug: "${slug}", kind, data}. An app may also EXPOSE MCP tools (exposes: declarative bindings over its records, exponential.<noun>.<op> operations or a team MCP server in upstreams) that a member's MCP client runs as that member; the catalog documents the format. A person may attach a spreadsheet (CSV, TSV or xlsx) to fill the app: follow the import recipe (the exponential-apps skill's references/import.md, docs/spec/apps.md §10.2): read it, map the columns onto a kind, show a dry run and ask, then records_upsert row by row in batches.`
}

/** The brief an app's EDITOR SESSION gets as the preface of its first
 *  message (every new or continued session): what the app is, the MCP
 *  calls that read and publish it, and the built-in skills it runs with. */
export function editorBrief(app: { name: string; slug: string; headVersion: number; description?: string | null }): string {
  const lines = [
    `You are the editor session of the Exponential app "${app.name}" (slug \`${app.slug}\`, ${versionLabel(app.headVersion)}).${app.description?.trim() ? ` It is for: ${app.description.trim()}` : ``}`,
    ...(isDraftApp(app)
      ? [`It is a new DRAFT with a placeholder name: on your FIRST exponential_apps {op: "upsert"} also pass name, icon, color and description that fit what the person asks for. The slug follows the new name; use the slug the upsert returns from then on.`]
      : []),
    `${editorWorkflow(app.slug)} The person sees the live preview beside this chat, so keep replies to one or two lines and save drafts early.`,
    `This session runs with the built-in skills ${EDITOR_SKILLS.join(`, `)}: read ${EDITOR_SKILLS[0]} (its references and the three worked example apps) before your first upsert.`,
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

/** VAPP-179: the preview's templates, sorted by id (a JSON layer, Postgres
 *  JSONB included, does not keep the declared order). */
export function previewTemplates(pkg: { templates?: Record<string, unknown> } | null | undefined): string[] {
  return Object.keys(pkg?.templates ?? {}).sort()
}

/** The template the preview shows: the picked one while the package has it,
 *  else the first (`templateCases`). */
export function previewTemplate(pkg: { templates?: Record<string, unknown> } | null | undefined, picked: string | null): string | null {
  const list = previewTemplates(pkg)
  return picked !== null && list.includes(picked) ? picked : (list[0] ?? null)
}

/** VAPP-198: one Run as option (`value` "" = me). */
export interface RunAsOption {
  value: string
  label: string
}

/** VAPP-198: the preview toolbar's Run as options (`runAsCases`): null
 *  unless the viewer is a team owner with another member; else Me, then
 *  every other member by name (case-insensitive, then id; no name = id). */
export function runAsOptions(members: readonly { userId: string; name?: string | null; role: string }[], me: string | null, role: string | null): RunAsOption[] | null {
  if (role !== `owner`) return null
  const others = members
    .filter((m) => m.userId !== me)
    .map((m) => ({ value: m.userId, label: m.name?.trim() ? m.name : m.userId }))
    .sort((a, b) => {
      const x = a.label.toLowerCase()
      const y = b.label.toLowerCase()
      return x < y ? -1 : x > y ? 1 : a.value < b.value ? -1 : a.value > b.value ? 1 : 0
    })
  if (others.length === 0) return null
  return [{ value: ``, label: fixture.copy.runAs.me }, ...others]
}

/** #VAPP-220 (`automationRowCases`): one row of the Settings tab's
 *  Automations group, from tRPC `appAccess.appTools` `automations`. */
export interface AutomationRowInput {
  name: string
  on: { schedule: { cron: string; tz?: string } } | { record: { kind: string; op: `create` | `update` | `delete` } } | { event: string }
  paused: string | null
  lastStatus: string | null
}

export type AutomationStatus = `paused` | `failed` | `ok` | `never`

export function automationRow(row: AutomationRowInput): { trigger: string; status: AutomationStatus; label: string } {
  const copy = fixture.copy.automations
  const fill = (text: string, values: Record<string, string>) => text.replace(/\{(\w+)\}/g, (_, key: string) => values[key] ?? ``)
  const on = row.on
  const trigger =
    `schedule` in on
      ? fill(on.schedule.tz ? copy.scheduleZone : copy.schedule, { cron: on.schedule.cron, tz: on.schedule.tz ?? `` })
      : `record` in on
        ? fill(copy[on.record.op], { kind: on.record.kind })
        : fill(copy.event, { event: on.event })
  const status: AutomationStatus = row.paused ? `paused` : row.lastStatus === null ? `never` : row.lastStatus !== `ok` ? `failed` : `ok`
  return { trigger, status, label: copy[status] }
}
