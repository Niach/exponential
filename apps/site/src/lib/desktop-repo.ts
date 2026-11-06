/* Exponential Desktop's package repository. The host lives in ONE file,
   apps/ede/packaging/repo.env (KEY=VALUE lines, PKG_HOST); every URL the install
   docs show is built from it here. A missing file or key fails the build. */
import repoEnv from "../../../ede/packaging/repo.env?raw"

function parse(text: string): Record<string, string> {
  const out: Record<string, string> = {}
  for (const raw of text.split(`\n`)) {
    const line = raw.trim()
    if (!line || line.startsWith(`#`)) continue
    const eq = line.indexOf(`=`)
    if (eq < 1) continue
    out[line.slice(0, eq).trim()] = line.slice(eq + 1).trim().replace(/^["']|["']$/g, ``)
  }
  return out
}

const env = parse(repoEnv)

function required(key: string): string {
  const value = env[key]
  if (!value) throw new Error(`apps/ede/packaging/repo.env: ${key} is missing`)
  return value
}

export const PKG_HOST = required(`PKG_HOST`)

const BASE = `https://${PKG_HOST}`

/** The signing key every repository uses, and its fingerprint (repo.env: PKG_KEY_FPR). */
const FPR = required(`PKG_KEY_FPR`)
export const SIGNING_KEY = {
  url: `${BASE}/exponential.asc`,
  fingerprint: FPR.replace(/(.{4})/g, `$1 `).trim().replace(/^((?:\S{4} ){4}\S{4}) /, `$1  `),
  id: FPR,
} as const

/** apt suites (repo.env: PKG_APT_SUITES). */
export const APT_SUITES = required(`PKG_APT_SUITES`).split(/\s+/)

/** The setup script as served: the host, key and suites filled in. */
export function renderSetupScript(template: string): string {
  return template.replaceAll(`@PKG_HOST@`, PKG_HOST).replaceAll(`@PKG_KEY_FPR@`, FPR).replaceAll(`@PKG_APT_SUITES@`, APT_SUITES.join(` `))
}

export const DESKTOP_REPO = {
  debian: { url: `${BASE}/apt` },
  fedora: { repofile: `${BASE}/rpm/exponential.repo` },
  arch: { server: `${BASE}/arch/$arch`, mirrorlist: `/etc/pacman.d/exponential-mirrorlist` },
} as const

/** The one-line setup script, served by this site at /desktop/setup.sh. */
export function setupScriptUrl(site: string | URL | undefined): string {
  if (!site) throw new Error(`astro.config: site is unset, so the setup script has no URL`)
  return new URL(`/desktop/setup.sh`, site).href
}

/** The deb822 source for one suite (noble, resolute, trixie). */
export function debSources(suite: string): string {
  return [`Types: deb`, `URIs: ${DESKTOP_REPO.debian.url}`, `Suites: ${suite}`, `Components: main`, `Signed-By: /etc/apt/keyrings/exponential.asc`].join(`\n`)
}
