// Server-only lookup of the cloud dogfood widget key (the `Exponential App`
// config bootstrap-cloud creates on the public feedback team).
//
// This module is reached ONLY via a dynamic import inside the
// getRuntimeConfig serverFn handler. Keep it a small leaf with static
// imports: dynamically importing the big `@/db/schema` re-export module from
// the handler made rollup synthesize a chunk-wide namespace object that
// referenced an unimported start-server binding (attachRouterServerSsrUtils)
// and crashed every request in the production bundle.
import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { widgetConfigs, teams } from "@/db/schema"

export async function findDogfoodWidgetKey(): Promise<string | null> {
  const [row] = await db
    .select({ publicKey: widgetConfigs.publicKey })
    .from(widgetConfigs)
    .innerJoin(teams, eq(widgetConfigs.teamId, teams.id))
    .where(
      and(
        eq(teams.slug, `feedback`),
        eq(widgetConfigs.name, `Exponential App`),
        eq(widgetConfigs.enabled, true)
      )
    )
    .limit(1)
  return row?.publicKey ?? null
}
