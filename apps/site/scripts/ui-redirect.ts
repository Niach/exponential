/* ui.exponential.at: every request answers 301, straight to its final URL
   on exponential.at (one hop: the old /ui/ section's pages moved into the
   docs, src/redirects.ts). Root files keep their path; a path already under
   /ui/ is not prefixed twice. The Coolify app `exponential-ui-site` builds
   Dockerfile.ui-redirect (this file + src/redirects.ts on bun alpine, :3000):

     bun apps/site/scripts/ui-redirect.ts        # PORT defaults to 80 */
import { redirectFor } from "../src/redirects.ts"

const ORIGIN = `https://exponential.at`
const ROOT_FILES = /^\/(llms\.txt|sitemap\.xml|robots\.txt|favicon\.ico|logo-(light|dark)\.svg|apple-touch-icon\.png|icon-\d+\.png|(og|shots|fonts)\/.*)$/

export function redirectTarget(url: string): string {
  let pathname: string
  let search: string
  try {
    ;({ pathname, search } = new URL(url))
  } catch {
    return `${ORIGIN}/sdk/genui/`
  }
  if (ROOT_FILES.test(pathname)) return `${ORIGIN}${pathname}${search}`
  const path = pathname === `/ui` || pathname.startsWith(`/ui/`) ? pathname : `/ui${pathname === `/` ? `/` : pathname}`
  const moved = redirectFor(path)
  const final = moved ?? (path.endsWith(`/`) || /\.[a-z0-9]+$/i.test(path) ? path : `${path}/`)
  return `${ORIGIN}${final}${search}`
}

if (import.meta.main) {
  Bun.serve({
    port: Number(process.env.PORT ?? 80),
    development: false,
    fetch: (req) => Response.redirect(redirectTarget(req.url), 301),
  })
}
