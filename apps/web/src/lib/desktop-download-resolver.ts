// EXP-1693: which release a desktop download button lands on. A download must
// not depend on `releases/latest`: its link 404s whenever the newest release
// lacks the asset (the Windows .exe attaches late on every release; a macOS
// notarization flake once published one with no .dmg). So: the newest desktop-v*
// release that HAS the asset, from one cached, ETag-conditional release list.
// A click never waits on GitHub once a list is known (a stale one answers while
// it refreshes), and a GitHub outage falls back to the `latest` link.

import { parseVersionTuple } from "@/lib/client-version"
import { DESKTOP_ASSET_NAMES, type DesktopOs } from "@/lib/desktop-download"
import { githubFetch, type GithubClientDeps } from "@/lib/integrations/github-client"

const REPO = `Niach/exponential`
const RELEASES_API = `/repos/${REPO}/releases?per_page=100`
const LATEST_DOWNLOAD = `https://github.com/${REPO}/releases/latest/download`
const TAG_PREFIX = `desktop-v`
const REFRESH_MS = 5 * 60_000
const RETRY_MS = 60_000
/** How long the first click after a boot waits for GitHub. */
const FIRST_FETCH_WAIT_MS = 5_000

export interface GithubReleaseAsset {
  name: string
  browser_download_url: string
  state?: string
}

export interface GithubRelease {
  tag_name: string
  draft?: boolean
  prerelease?: boolean
  assets?: GithubReleaseAsset[]
}

type Targets = Partial<Record<DesktopOs, { url: string; version: [number, number, number] }>>

export function isDesktopOs(os: string): os is DesktopOs {
  return Object.hasOwn(DESKTOP_ASSET_NAMES, os)
}

function isNewer(a: [number, number, number], b: [number, number, number]): boolean {
  return a[0] !== b[0] ? a[0] > b[0] : a[1] !== b[1] ? a[1] > b[1] : a[2] > b[2]
}

/** Per OS, the highest-versioned published desktop release with the complete
 *  asset. GitHub lists by creation date and mixes every tag family. */
export function pickDesktopDownloads(releases: GithubRelease[]): Targets {
  const targets: Targets = {}
  for (const release of releases) {
    if (release.draft || release.prerelease || !release.tag_name.startsWith(TAG_PREFIX)) continue
    const version = parseVersionTuple(release.tag_name.slice(TAG_PREFIX.length))
    if (!version) continue
    for (const os of Object.keys(DESKTOP_ASSET_NAMES) as DesktopOs[]) {
      const asset = release.assets?.find(
        (candidate) =>
          candidate.name === DESKTOP_ASSET_NAMES[os] &&
          (candidate.state === undefined || candidate.state === `uploaded`)
      )
      const best = targets[os]
      if (asset && (!best || isNewer(version, best.version))) {
        targets[os] = { url: asset.browser_download_url, version }
      }
    }
  }
  return targets
}

let known: Targets | null = null
let nextRefreshAt = 0
let refreshing: Promise<void> | null = null

function refresh(deps: GithubClientDeps): Promise<void> {
  refreshing ??= (async () => {
    const now = deps.now ?? Date.now
    try {
      const res = await githubFetch(
        RELEASES_API,
        { headers: { accept: `application/vnd.github+json`, "user-agent": `exponential-web` } },
        deps
      )
      if (!res.ok) throw new Error(`GitHub answered ${res.status}`)
      const releases: unknown = await res.json()
      if (!Array.isArray(releases)) throw new Error(`GitHub answered no release list`)
      // An OS missing from this page keeps the release it had.
      known = { ...known, ...pickDesktopDownloads(releases as GithubRelease[]) }
      nextRefreshAt = now() + REFRESH_MS
    } catch (err) {
      nextRefreshAt = now() + RETRY_MS
      console.warn(`[desktop-download] release list failed: ${err instanceof Error ? err.message : String(err)}`)
    } finally {
      refreshing = null
    }
  })()
  return refreshing
}

export async function resolveDesktopDownload(
  os: DesktopOs,
  deps: GithubClientDeps = {}
): Promise<{ url: string; fromRelease: boolean }> {
  const now = deps.now ?? Date.now
  if (now() >= nextRefreshAt) {
    const pending = refresh(deps)
    if (!known) {
      let timer: ReturnType<typeof setTimeout> | undefined
      await Promise.race([
        pending,
        new Promise<void>((resolve) => {
          timer = setTimeout(resolve, FIRST_FETCH_WAIT_MS)
        }),
      ])
      clearTimeout(timer)
    }
  }
  const target = known?.[os]
  return target
    ? { url: target.url, fromRelease: true }
    : { url: `${LATEST_DOWNLOAD}/${DESKTOP_ASSET_NAMES[os]}`, fromRelease: false }
}

/** Test hook. */
export function resetDesktopDownloadCache(): void {
  known = null
  nextRefreshAt = 0
  refreshing = null
}
