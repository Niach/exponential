/* ui.exponential.at, after the merge into exponential.at (EXP-1392): every
   request answers 301 to the same path (and query) under exponential.at/ui/.
   The old site's root files (llms.txt, the sitemap, robots, icons, the OG
   card, the shot store, fonts) live at the root of exponential.at now, so
   they keep their path; a path already under /ui/ is not prefixed twice.
   The Coolify app `exponential-ui-site` runs this instead of serving a build:

     bun apps/marketing/ui/scripts/redirect.ts        # PORT defaults to 80 */
const ORIGIN = `https://exponential.at`
const ROOT_FILES = /^\/(llms\.txt|sitemap\.xml|robots\.txt|favicon\.ico|logo-(light|dark)\.svg|apple-touch-icon\.png|icon-\d+\.png|(og|shots|fonts)\/.*)$/

export function redirectTarget(url: string): string {
  let pathname: string
  let search: string
  try {
    ;({ pathname, search } = new URL(url))
  } catch {
    /* A malformed Host header: still a 301 home, never Bun's 500 page. */
    return `${ORIGIN}/ui/`
  }
  if (ROOT_FILES.test(pathname) || pathname.startsWith(`/ui/`)) return `${ORIGIN}${pathname}${search}`
  if (pathname === `/ui`) return `${ORIGIN}/ui/${search}`
  return `${ORIGIN}/ui${pathname}${search}`
}

if (import.meta.main) {
  Bun.serve({
    port: Number(process.env.PORT ?? 80),
    development: false,
    fetch: (req) => Response.redirect(redirectTarget(req.url), 301),
  })
}
