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
 *
 * iPadOS Safari masquerades as plain "Macintosh" with no ipad/mobile token —
 * the UA alone can't tell it from a Mac, so callers pass `maxTouchPoints`
 * (from `navigator.maxTouchPoints`; real Macs report 0, iPads report 5).
 */
export function desktopDownloadHref(
  userAgent: string,
  maxTouchPoints = 0
): string {
  const ua = userAgent.toLowerCase()
  // Mobile first: Android UAs also contain "linux".
  if (/android|iphone|ipad|ipod|mobile/.test(ua)) return DESKTOP_RELEASES_URL
  if (ua.includes(`windows`)) return DESKTOP_ASSET_URLS.windows
  if (ua.includes(`mac os`) || ua.includes(`macintosh`)) {
    // Desktop-mode iPadOS: mac UA + touch screen ⇒ no .dmg to install.
    if (maxTouchPoints > 1) return DESKTOP_RELEASES_URL
    return DESKTOP_ASSET_URLS.macos
  }
  if (ua.includes(`linux`) || ua.includes(`x11`)) return DESKTOP_ASSET_URLS.linux
  return DESKTOP_RELEASES_URL
}
