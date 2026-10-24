/* ui.exponential.at, after the merge into exponential.at (EXP-1392): every
   request answers 301 to the same path (and query) under exponential.at/ui/.
   The Coolify app `exponential-ui-site` runs this instead of serving a build:

     bun apps/marketing/ui/scripts/redirect.ts        # PORT defaults to 80 */
const TARGET = `https://exponential.at/ui`

export function redirectTarget(url: string): string {
  const { pathname, search } = new URL(url)
  return `${TARGET}${pathname}${search}`
}

if (import.meta.main) {
  Bun.serve({
    port: Number(process.env.PORT ?? 80),
    fetch: (req) => Response.redirect(redirectTarget(req.url), 301),
  })
}
