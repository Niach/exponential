// Desktop-app download links (EXP-68). Asset names are fixed by
// .github/workflows/build-desktop.yml and shared with the desktop
// self-updater (`updater::expected_asset_name`): exactly one asset per OS —
// notarized `.dmg`, raw `.exe`, raw `.AppImage`.

const RELEASES = `https://github.com/Niach/exponential/releases`

export const DESKTOP_RELEASES_URL = `${RELEASES}/latest`

const DOWNLOAD_BASE = `${RELEASES}/latest/download`

export const DESKTOP_ASSET_URLS = {
  macos: `${DOWNLOAD_BASE}/Exponential-production.dmg`,
  windows: `${DOWNLOAD_BASE}/Exponential-production-x86_64-windows.exe`,
  linux: `${DOWNLOAD_BASE}/Exponential-production-x86_64.AppImage`,
} as const

/**
 * The best download target for a user agent: the platform's release asset on
 * a desktop OS, the releases page everywhere else (mobile / unknown — there
 * is nothing to install there, but the page explains the app).
 */
export function desktopDownloadHref(userAgent: string): string {
  const ua = userAgent.toLowerCase()
  // Mobile first: Android UAs also contain "linux", and iPadOS Safari
  // masquerades as "macintosh" but is detectable via its touch hints.
  if (/android|iphone|ipad|ipod|mobile/.test(ua)) return DESKTOP_RELEASES_URL
  if (ua.includes(`windows`)) return DESKTOP_ASSET_URLS.windows
  if (ua.includes(`mac os`) || ua.includes(`macintosh`)) {
    return DESKTOP_ASSET_URLS.macos
  }
  if (ua.includes(`linux`) || ua.includes(`x11`)) return DESKTOP_ASSET_URLS.linux
  return DESKTOP_RELEASES_URL
}
