/* Every origin and external URL the Exponential UI pages (exponential.at/ui)
   link. The site ships a CSP (apps/marketing/public/serve.json): a new origin
   the browser has to FETCH must be added there in the same change; href-only
   origins need nothing. */
export const SITE_ORIGIN = `https://exponential.at`
export const SITE_NAME = `eUI`
/** Where the section lives on exponential.at (ui.exponential.at redirects here). */
export const UI_HOME = `/ui/`

const REPO = `https://github.com/Niach/exponential`

export const LINKS = {
  marketing: `/`,
  app: `https://app.exponential.at/`,
  styleguide: `https://styleguide.exponential.at/`,
  repo: REPO,
  /** A path in the repo on master (a file or a directory). */
  source: (path: string) => `${REPO}/tree/master/${path}`,
  a2ui: `https://a2ui.org/`,
} as const
