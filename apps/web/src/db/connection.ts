import "@dotenvx/dotenvx/config"
import { drizzle } from "drizzle-orm/node-postgres"
import { ResilientPool } from "./resilient-pool"

const databaseUrl = process.env.DATABASE_URL
if (!databaseUrl) {
  throw new Error(`DATABASE_URL is not set`)
}
// Exported for the admin performance page's pool gauges (EXP-553) — read
// totalCount/idleCount/waitingCount only, never query through it directly.
// ResilientPool: fresh DNS per connection, connect retry with backoff, idle
// eviction on connection errors, so a DB restart (new container IP) heals
// without restarting the web app.
export const pool = new ResilientPool(
  { connectionString: databaseUrl },
  {
    attempts: 5,
    baseDelayMs: 100,
    maxDelayMs: 2000,
    onRetry: (err, attempt, delayMs) => {
      const code = (err as { code?: string }).code ?? (err as Error).message
      console.warn(`[db] connect failed (${code}), retry ${attempt} in ${delayMs}ms`)
    },
  }
)
export const db = drizzle({ client: pool, casing: `snake_case` })
