/* Single source of truth for every external URL on the marketing site.
   Swap the download placeholders for real store / release-asset URLs at launch. */

const APP = `https://app.exponential.at`
const REPO = `https://github.com/Niach/exponential`

export const LINKS = {
  downloadPage: `/download/`,
  app: {
    register: `${APP}/auth/register`,
    login: `${APP}/auth/login`,
    mcp: `${APP}/api/mcp`,
    // Enterprise contact-form endpoint (pricing page). JSON POST; the web
    // app answers the CORS preflight — see apps/web /api/contact.
    contact: `${APP}/api/contact`,
  },
  github: {
    repo: REPO,
    releases: `${REPO}/releases`,
  },
  downloads: {
    // GitHub Releases `latest` assets — published by build-desktop.yml on
    // desktop-v* tags. Asset names are fixed by the release pipeline and
    // shared with the desktop self-updater (EXP-68: exactly one asset per
    // OS — notarized .dmg, raw .exe, raw .AppImage).
    macos: `${REPO}/releases/latest/download/Exponential-production.dmg`,
    windows: `${REPO}/releases/latest/download/Exponential-production-x86_64-windows.exe`,
    linux: `${REPO}/releases/latest/download/Exponential-production-x86_64.AppImage`,
    // Mobile ships as public betas until the store listings are approved.
    // iOS: TestFlight external beta; Android: Google Play open (beta) track.
    ios: `https://testflight.apple.com/join/JMpJKZEB`,
    android: `https://play.google.com/store/apps/details?id=at.exponential`,
  },
} as const

/* The real feedback widget, embedded live on the marketing site — visitors
   experience step 1 of the loop for real. The submission lands on the public
   feedback board. This is the cloud bootstrap's `Exponential App` config key
   (a PUBLIC widget key by design — it ships in page snippets). Its domain
   allowlist is exponential.at / www.exponential.at / app.exponential.at;
   self-hosted instances don't embed it — they redirect to the cloud board. */
export const WIDGET = {
  key: `expw_ATLHZ5hFiV5CqApwbCawPh72bPkpbHUp`,
  loader: `${APP}/widget/v1/loader.js`,
} as const
