import { createFileRoute } from "@tanstack/react-router"

// Served dynamically (not a static public/ file) so the Sitemap line always
// points at the running instance's own origin — correct for self-hosted
// deployments too. Public feedback boards are the only indexable surface; the
// rest of the app carries a per-page noindex from __root.tsx.
export const Route = createFileRoute(`/robots.txt`)({
  server: {
    handlers: {
      GET: ({ request }) => {
        const origin = (
          process.env.BETTER_AUTH_URL || new URL(request.url).origin
        ).replace(/\/$/, ``)
        const body = `User-agent: *\nAllow: /\nSitemap: ${origin}/sitemap.xml\n`
        return new Response(body, {
          headers: {
            "Content-Type": `text/plain; charset=utf-8`,
            "Cache-Control": `public, max-age=3600`,
          },
        })
      },
    },
  },
})
