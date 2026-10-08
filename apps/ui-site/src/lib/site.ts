/* Every origin and external URL ui.exponential.at links. The site ships a CSP
   (public/serve.json): a new origin the browser has to FETCH must be added
   there in the same change; href-only origins need nothing. */
export const SITE_ORIGIN = `https://ui.exponential.at`
export const SITE_NAME = `Exponential UI`

const REPO = `https://github.com/Niach/exponential`

export const LINKS = {
  marketing: `https://exponential.at/`,
  app: `https://app.exponential.at/`,
  styleguide: `https://styleguide.exponential.at/`,
  repo: REPO,
  /** A path in the repo on master (a file or a directory). */
  source: (path: string) => `${REPO}/tree/master/${path}`,
  a2ui: `https://a2ui.org/`,
} as const
