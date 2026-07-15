import { createFileRoute } from "@tanstack/react-router"
import { and, eq, inArray, isNull } from "drizzle-orm"

// Server-only sitemap of the public surface: every unarchived feedback board
// plus its non-archived issues. Built from getPublicProjectScope() so it stays
// in lockstep with what anonymous shape proxies actually expose. Cached for 1h
// (unfurlers/crawlers hit it rarely); an empty scope yields a valid empty
// urlset, never an error.

async function getDb() {
  const { db } = await import(`@/db/connection`)
  return db
}

function xmlEscape(value: string): string {
  return value
    .replace(/&/g, `&amp;`)
    .replace(/</g, `&lt;`)
    .replace(/>/g, `&gt;`)
    .replace(/"/g, `&quot;`)
    .replace(/'/g, `&apos;`)
}

type SitemapCache = { xml: string; expires: number }
const SITEMAP_TTL_MS = 60 * 60 * 1000
let sitemapCache: SitemapCache | undefined

async function buildSitemap(origin: string): Promise<string> {
  const { getPublicProjectScope } = await import(`@/lib/auth/membership`)
  const scope = await getPublicProjectScope()

  const urls: { loc: string; lastmod: string }[] = []

  if (scope.projectIds.length > 0) {
    const db = await getDb()
    const { issues, projects, workspaces } = await import(`@/db/schema`)

    const projectRows = await db
      .select({
        id: projects.id,
        slug: projects.slug,
        updatedAt: projects.updatedAt,
        workspaceSlug: workspaces.slug,
      })
      .from(projects)
      .innerJoin(workspaces, eq(projects.workspaceId, workspaces.id))
      .where(inArray(projects.id, scope.projectIds))

    for (const project of projectRows) {
      urls.push({
        loc: `${origin}/t/${project.workspaceSlug}/projects/${project.slug}`,
        lastmod: project.updatedAt.toISOString(),
      })
    }

    const byId = new Map(projectRows.map((p) => [p.id, p]))
    const issueRows = await db
      .select({
        projectId: issues.projectId,
        identifier: issues.identifier,
        updatedAt: issues.updatedAt,
      })
      .from(issues)
      .where(
        and(
          inArray(issues.projectId, scope.projectIds),
          isNull(issues.archivedAt)
        )
      )

    for (const issue of issueRows) {
      const project = byId.get(issue.projectId)
      if (!project) continue
      urls.push({
        loc: `${origin}/t/${project.workspaceSlug}/projects/${project.slug}/issues/${issue.identifier}`,
        lastmod: issue.updatedAt.toISOString(),
      })
    }
  }

  const body = urls
    .map(
      (u) =>
        `  <url><loc>${xmlEscape(u.loc)}</loc><lastmod>${u.lastmod}</lastmod></url>`
    )
    .join(`\n`)

  return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${body}\n</urlset>\n`
}

export const Route = createFileRoute(`/sitemap.xml`)({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const origin = (
          process.env.BETTER_AUTH_URL || new URL(request.url).origin
        ).replace(/\/$/, ``)

        const now = Date.now()
        if (!sitemapCache || sitemapCache.expires <= now) {
          const xml = await buildSitemap(origin)
          sitemapCache = { xml, expires: now + SITEMAP_TTL_MS }
        }

        return new Response(sitemapCache.xml, {
          headers: {
            "Content-Type": `application/xml; charset=utf-8`,
            "Cache-Control": `public, max-age=3600`,
          },
        })
      },
    },
  },
})
