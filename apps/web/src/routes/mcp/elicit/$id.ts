import { createFileRoute } from "@tanstack/react-router"
import { and, eq, gt } from "drizzle-orm"
import fixture from "@exp/emcp/fixtures/elicitation.json" with { type: "json" }
import { db } from "@/db/connection"
import { mcpElicitations } from "@/db/schema"
import { auth } from "@/lib/auth"

// #NEW-147 (packages/emcp fixtures/elicitation.json URL MODE): the ONE page
// an MCP URL elicitation opens, `${APP_URL}/mcp/elicit/<id>`. The id is
// opaque (a server-side `mcp_elicitations` row: purpose, member, target,
// expiry), never a pre-authenticated link: the opener signs in, and only the
// member the call was made for is sent on to where the person acts (the
// billing page, the GitHub App's install page, the MCP server's connect page).
const ID_RE = new RegExp(fixture.url.idPattern)

const page = (status: number, text: string) =>
  new Response(`<!doctype html><meta charset="utf-8"><title>Exponential</title><p>${text}</p>`, {
    status,
    headers: { "content-type": `text/html; charset=utf-8`, "cache-control": `no-store` },
  })

const redirect = (location: string) => new Response(null, { status: 302, headers: { location, "cache-control": `no-store` } })

/** A target is a same-origin path, or the GitHub App's page on github.com. */
function safeTarget(target: string): string | null {
  if (target.startsWith(`/`) && !target.startsWith(`//`)) return target
  try {
    const url = new URL(target)
    return url.protocol === `https:` && url.hostname === `github.com` ? url.href : null
  } catch {
    return null
  }
}

export const Route = createFileRoute(`/mcp/elicit/$id`)({
  server: {
    handlers: {
      GET: async ({ request, params }) => {
        const id = params.id
        if (!ID_RE.test(id)) return page(404, `This link is not valid.`)
        const [row] = await db
          .select()
          .from(mcpElicitations)
          .where(and(eq(mcpElicitations.id, id), gt(mcpElicitations.expiresAt, new Date())))
          .limit(1)
        if (!row) return page(404, `This link has expired. Go back to your assistant and try again.`)
        const session = await auth.api.getSession({ headers: request.headers })
        if (!session?.user) return redirect(`/auth/login?redirect=${encodeURIComponent(`${fixture.url.path}${id}`)}`)
        if (session.user.id !== row.userId) {
          return page(403, `This link was made for another account. Sign in as the person who asked, or go back to your assistant.`)
        }
        const target = safeTarget(row.target)
        if (!target) return page(404, `This link is not valid.`)
        await db.update(mcpElicitations).set({ openedAt: new Date() }).where(eq(mcpElicitations.id, id))
        return redirect(target)
      },
    },
  },
})
