import "@dotenvx/dotenvx/config"
import { drizzle } from "drizzle-orm/node-postgres"
import { Pool } from "pg"

const databaseUrl = process.env.DATABASE_URL
if (!databaseUrl) {
  throw new Error(`DATABASE_URL is not set`)
}
// Exported for the admin performance page's pool gauges (EXP-553) — read
// totalCount/idleCount/waitingCount only, never query through it directly.
export const pool = new Pool({ connectionString: databaseUrl })
export const db = drizzle({ client: pool, casing: `snake_case` })
