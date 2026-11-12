import { createFileRoute } from "@tanstack/react-router"
import { sql } from "drizzle-orm"

// Lazy import (same pattern as lib/auth/membership.ts) so loading this route
// module never eagerly opens a DB pool at build/import time.
async function getDb() {
  const { db } = await import(`@/db/connection`)
  return db
}

const DB_PROBE_TIMEOUT_MS = 3000

// Liveness/readiness probe for Docker HEALTHCHECK / orchestrators / uptime
// monitors. 200 only when the database answers; Electric reachability is
// reported but NOT gating (the app serves cached UI + tRPC without it, and a
// flapping Electric shouldn't restart-loop the web container).
export const Route = createFileRoute(`/api/health`)({
  server: {
    handlers: {
      GET: async () => {
        // Bounded: while the pool is retrying a vanished DB (restart, new IP)
        // the probe answers 503 within DB_PROBE_TIMEOUT_MS instead of hanging,
        // and flips back to 200 on the first query the healed pool answers.
        let dbOk = false
        let timer: ReturnType<typeof setTimeout> | undefined
        try {
          const db = await getDb()
          await Promise.race([
            db.execute(sql`select 1`),
            new Promise((_, reject) => {
              timer = setTimeout(() => reject(new Error(`db probe timeout`)), DB_PROBE_TIMEOUT_MS)
            }),
          ])
          dbOk = true
        } catch {
          dbOk = false
        } finally {
          clearTimeout(timer)
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
