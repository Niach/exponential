import { drizzle } from "drizzle-orm/node-postgres"
import pg from "pg"

/** The migration script's own connection (never the server's pool). `readOnly`
 *  (a dry run): Postgres refuses every write of the session. */
export function connectMigration(connectionString: string, options: { readOnly: boolean }) {
  const pool = new pg.Pool({ connectionString, max: 1 })
  if (options.readOnly) pool.on(`connect`, (client) => void client.query(`SET default_transaction_read_only = on`))
  return { db: drizzle({ client: pool, casing: `snake_case` }), close: () => pool.end() }
}
