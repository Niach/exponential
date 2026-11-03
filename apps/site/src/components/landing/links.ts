/* Every external URL of the landing area (from apps/marketing/src/lib/links.ts).
   An origin the browser FETCHES (script, XHR, embed) must also be in the
   CSP of serve.base.json; href targets need nothing. */
const APP = `https://app.exponential.at`
const REPO = `https://github.com/Niach/exponential`

export const LINKS = {
  app: {
    /* Sign-up and login are one page. */
    login: `${APP}/auth/login`,
    /* The contact form's JSON endpoint (CORS answered by apps/web /api/contact). */
    contact: `${APP}/api/contact`,
  },
  github: { repo: REPO, releases: `${REPO}/releases`, license: `${REPO}/blob/master/LICENSE` },
  downloads: {
    macos: `${REPO}/releases/latest/download/Exponential-production.dmg`,
    windows: `${REPO}/releases/latest/download/Exponential-production-x86_64-windows.exe`,
    linux: `${REPO}/releases/latest/download/Exponential-production-x86_64.AppImage`,
    ios: `https://apps.apple.com/app/exponential-vibecode-together/id6788189402`,
    android: `https://play.google.com/store/apps/details?id=at.exponential`,
  },
} as const
