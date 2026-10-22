// VAPP-110: Exponential apps, the pure reference every client mirrors
// (fixtures/apps.json is the spec; desktop `domain::apps`, iOS
// `Apps.swift`, Android `domain/Apps.kt` replay its cases).

import fixture from "../fixtures/apps.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }

export const APPS_COPY = fixture.copy
export const APPS_BUILTIN = fixture.builtin
export const APPS_PANES = fixture.panes
export const APPS_PREVIEW = fixture.preview
export const APPS_TEST_IDS = fixture.testIds

export const APP_SLUG_MAX = contract.apps.slugMax
export const APP_NAME_MAX = contract.apps.nameMax
export const APP_DESCRIPTION_MAX = contract.apps.descriptionMax
export const APP_NOTE_MAX = contract.apps.noteMax
export const APP_PACKAGE_MAX_BYTES = contract.apps.packageMaxBytes
export const APP_TEMPLATE_MAX_COMPONENTS = contract.apps.templateMaxComponents
export const APP_SLUG_PATTERN = new RegExp(contract.apps.slugPattern)

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
  builtin: boolean
}

/** The list order: the built-in app first, then by name (case-insensitive). */
export function sortApps<T extends AppListLike>(apps: readonly T[]): T[] {
  return [...apps].sort((a, b) => {
    if (a.builtin !== b.builtin) return a.builtin ? -1 : 1
    return a.name.localeCompare(b.name, undefined, { sensitivity: `base` })
  })
}

/** The brief an app's EDITOR SESSION gets as the preface of its first
 *  message (every new or continued session): what the app is and the MCP
 *  calls that read and publish it. */
export function editorBrief(app: { name: string; slug: string; headVersion: number; description?: string | null }): string {
  const lines = [
    `You are the editor session of the Exponential app "${app.name}" (slug \`${app.slug}\`, ${versionLabel(app.headVersion)}).${app.description?.trim() ? ` It is for: ${app.description.trim()}` : ``}`,
    `An app is a declarative Exponential UI package (manifest + A2UI templates + bindings + a function allowlist; data, never code). Read the current package with exponential_apps {op: "get", slug: "${app.slug}"}; learn the components, bindings and functions with exponential_apps {op: "catalog"}; publish every change as a new version with exponential_apps {op: "upsert", slug: "${app.slug}", package, note}. The person sees the live preview beside this chat, so keep replies to one or two lines and publish early.`,
  ]
  return lines.join(`\n`)
}
