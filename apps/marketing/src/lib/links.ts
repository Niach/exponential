/* Single source of truth for every external URL on the marketing site.
   Swap the download placeholders for real store / release-asset URLs at launch. */

const APP = `https://app.exponential.at`
const REPO = `https://github.com/Niach/exponential`

export const LINKS = {
  downloadPage: `/download/`,
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
    // GitHub Releases `latest` assets — published by build-desktop.yml on desktop-v* tags.
    // Asset names are fixed by the release pipeline (masterplan P4.d).
    // macOS ships the ad-hoc-signed zip today; switch to
    // Exponential-production.dmg once MACOS_CERT_P12 lands (the notarized
    // .dmg replaces the .zip asset in the same release).
    macos: `${REPO}/releases/latest/download/exp-desktop-production.zip`,
    windows: `${REPO}/releases/latest/download/Exponential-production-x86_64-windows.zip`,
    linux: `${REPO}/releases/latest/download/Exponential-production-x86_64.AppImage`,
    // Mobile stores are placeholders until the apps are approved — see DownloadSection.
    // TODO(launch): App Store URL
    ios: `${REPO}/releases`,
    // TODO(launch): Play Store URL
    android: `${REPO}/releases`,
  },
} as const

/* The real feedback widget, embedded live on the marketing site — visitors
   experience step 1 of the loop for real. The submission lands on the public
   feedback board. Swap the placeholder key for the demo `expw_` config's real
   key at launch (release checklist §13) — this const is the single source. */
export const WIDGET = {
  // TODO(launch §13): replace with the marketing-demo widget config's real key.
  key: `expw_MARKETING_DEMO_KEY`,
  loader: `${APP}/widget/v1/loader.js`,
} as const
