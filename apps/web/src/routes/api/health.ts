import { createFileRoute } from "@tanstack/react-router"
import { sql } from "drizzle-orm"

// Lazy import (same pattern as lib/auth/membership.ts) so loading this route
// module never eagerly opens a DB pool at build/import time.
async function getDb() {
  const { db } = await import(`@/db/connection`)
  return db
}

// Liveness/readiness probe for Docker HEALTHCHECK / orchestrators / uptime
// monitors. 200 only when the database answers; Electric reachability is
// reported but NOT gating (the app serves cached UI + tRPC without it, and a
// flapping Electric shouldn't restart-loop the web container).
export const Route = createFileRoute(`/api/health`)({
  server: {
    handlers: {
      GET: async () => {
        let dbOk = false
        try {
          const db = await getDb()
          await db.execute(sql`select 1`)
          dbOk = true
        } catch {
          dbOk = false
        }

        let electricOk: boolean | undefined
        const electricUrl = process.env.ELECTRIC_URL
        if (electricUrl) {
          try {
            const res = await fetch(`${electricUrl.replace(/\/$/, ``)}/v1/health`, {
              signal: AbortSignal.timeout(2000),
            })
            electricOk = res.ok
          } catch {
            electricOk = false
          }
        }

        return Response.json(
          { ok: dbOk, db: dbOk, electric: electricOk },
          { status: dbOk ? 200 : 503 }
        )
      },
    },
  },
})
