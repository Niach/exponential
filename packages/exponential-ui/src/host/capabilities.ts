// VAPP-260 (format version 5, docs/package-format.md "Capabilities", spec
// docs/spec/apps.md §2.2.2): a package's CAPABILITY MANIFEST, the ceiling of
// what its UI and agents may reach through the Engine's broker. Only the
// declaration lives here; the catalog's tools and consent are the Engine's
// (packages/eengine fixtures/capabilities.json). Pure; the Rust core mirrors
// it (host::capabilities).

import type { PackageIssue } from "./package"

export const MAX_CAPABILITIES = 16
export const MAX_CAPABILITY_WHY = 200
/** The scope ids a manifest may name (catalog/host.json `package.capabilities.scopes`). */
export const CAPABILITY_SCOPES: readonly string[] = [
  `camera.capture`,
  `clipboard.read`,
  `clipboard.write`,
  `files.read`,
  `files.write`,
  `focus.window`,
  `http.fetch`,
  `launch.app`,
  `location.read`,
  `notifications.post`,
  `photos.read`,
  `print.pdf`,
  `share.send`,
  `windows.read`,
]
/** The scopes whose entry lists its `origins` (and only these may). */
export const CAPABILITY_ORIGIN_SCOPES: readonly string[] = [`http.fetch`]
export const CAPABILITY_ORIGIN_PATTERN = /^https:\/\/[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*(:[0-9]{1,5})?$/

export interface PackageCapability {
  id: string
  why: string
  origins?: string[]
}

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const ENTRY_KEYS = [`id`, `why`, `origins`]

/** Every problem with a package's `capabilities` (unset = none). */
export function validatePackageCapabilities(pkg: Record<string, unknown>): PackageIssue[] {
  if (pkg.capabilities === undefined) return []
  if (!Array.isArray(pkg.capabilities)) return [{ path: `/capabilities`, message: `capabilities is a list of {id, why, origins?}` }]
  const issues: PackageIssue[] = []
  if (pkg.capabilities.length > MAX_CAPABILITIES) issues.push({ path: `/capabilities`, message: `at most ${MAX_CAPABILITIES} capabilities` })
  const seen = new Set<string>()
  pkg.capabilities.forEach((entry, i) => {
    const at = `/capabilities/${i}`
    if (!isObject(entry)) {
      issues.push({ path: at, message: `a capability is {id, why, origins?}` })
      return
    }
    for (const key of Object.keys(entry).sort()) if (!ENTRY_KEYS.includes(key)) issues.push({ path: `${at}/${key}`, message: `unknown key ${key}` })
    const id = entry.id
    if (typeof id !== `string` || !CAPABILITY_SCOPES.includes(id)) issues.push({ path: `${at}/id`, message: `unknown capability ${typeof id === `string` ? id : JSON.stringify(id)}` })
    else if (seen.has(id)) issues.push({ path: `${at}/id`, message: `duplicate capability ${id}` })
    else seen.add(id)
    if (typeof entry.why !== `string` || !entry.why.trim()) issues.push({ path: `${at}/why`, message: `why is a required string` })
    else if (Array.from(entry.why).length > MAX_CAPABILITY_WHY) issues.push({ path: `${at}/why`, message: `why is at most ${MAX_CAPABILITY_WHY} characters` })
    const wantsOrigins = typeof id === `string` && CAPABILITY_ORIGIN_SCOPES.includes(id)
    if (entry.origins === undefined) {
      if (wantsOrigins) issues.push({ path: `${at}/origins`, message: `${id} lists its origins` })
    } else if (!wantsOrigins) issues.push({ path: `${at}/origins`, message: `origins are only for ${CAPABILITY_ORIGIN_SCOPES.join(`, `)}` })
    else if (!Array.isArray(entry.origins) || !entry.origins.length) issues.push({ path: `${at}/origins`, message: `origins is a non-empty list` })
    else {
      const origins = new Set<string>()
      entry.origins.forEach((origin, j) => {
        if (typeof origin !== `string` || !CAPABILITY_ORIGIN_PATTERN.test(origin)) issues.push({ path: `${at}/origins/${j}`, message: `an origin is https://host[:port]` })
        else if (origins.has(origin)) issues.push({ path: `${at}/origins/${j}`, message: `duplicate origin ${origin}` })
        else origins.add(origin)
      })
    }
  })
  return issues
}
