import "@dotenvx/dotenvx/config"
import { defineConfig } from "drizzle-kit"

export default defineConfig({
  out: `./src/db/out`,
  schema: `../../packages/db-schema/src/schema.ts`,
  dialect: `postgresql`,
  casing: `snake_case`,
  dbCredentials: {
    url: process.env.DATABASE_URL!,
  },
})
