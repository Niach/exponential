import { createFileRoute } from "@tanstack/react-router"

// The app instance is NOT a search surface: nothing here should be indexed,
// public feedback boards included (their content is user-submitted feedback,
// not marketing copy — the indexable product surface is the marketing site).
//
// Crawling is deliberately ALLOWED rather than `Disallow: /`, because:
//   1. `noindex` only de-indexes a page a crawler is allowed to FETCH. A blanket
//      Disallow hides the noindex, so an externally-linked board URL can linger
//      in the index as a URL-only entry.
//   2. Social unfurlers (Twitterbot, facebookexternalhit) honour robots.txt, and
//      a Disallow would kill the OG/Twitter previews the public board share
//      buttons exist to produce.
// Indexing is prevented by the app-wide `noindex` (meta from __root.tsx +
// X-Robots-Tag from server-bun.ts), which every crawler sees on every page.
//
// Served dynamically (not a static public/ file) so the rules always describe
// the running instance — correct for self-hosted deployments too. No Sitemap
// line: an instance with no indexable surface has nothing to submit.
export const Route = createFileRoute(`/robots.txt`)({
  server: {
    handlers: {
      GET: () => {
        const body = [
          `User-agent: *`,
          // No indexable content anywhere; skip the machinery crawlers gain
          // nothing from fetching.
          `Disallow: /api/`,
          `Allow: /`,
          ``,
        ].join(`\n`)
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
