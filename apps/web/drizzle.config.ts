import "@dotenvx/dotenvx/config"
import { defineConfig } from "drizzle-kit"

export default defineConfig({
  out: `./src/db/out`,
  schema: `../../packages/db-schema/src/schema.ts`,
  dialect: `postgresql`,
  casing: `snake_case`,
  // #NEW-134: `issue_embeddings` exists only on a Postgres with pgvector and
  // is created at boot (src/db/out/custom/0002), never by a migration: push
  // and introspection must not see it (push would offer to drop it).
  tablesFilter: [`!issue_embeddings`],
  dbCredentials: {
    url: process.env.DATABASE_URL!,
  },
})
