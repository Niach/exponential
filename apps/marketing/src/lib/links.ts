/* Single source of truth for every external URL on the marketing site.
   Swap the download placeholders for real store / release-asset URLs at launch. */

const APP = `https://app.exponential.at`
const REPO = `https://github.com/Niach/exponential`

export const LINKS = {
  app: {
    register: `${APP}/auth/register`,
    login: `${APP}/auth/login`,
    feedback: `${APP}/feedback`,
    mcp: `${APP}/api/mcp`,
  },
  github: {
    repo: REPO,
    releases: `${REPO}/releases`,
  },
  downloads: {
    // TODO(launch): point at the signed .dmg release asset
    macos: `${REPO}/releases`,
    // TODO(launch): point at the Flatpak / AppImage release asset
    linux: `${REPO}/releases`,
    // TODO(launch): App Store URL
    ios: `${REPO}/releases`,
    // TODO(launch): Play Store URL
    android: `${REPO}/releases`,
  },
} as const
