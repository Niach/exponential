// #EXP-1614: `repositories.provider_config` (server-only) holds the
// provider's settings; GitHub's is `{installationId}`. The `installation_id`
// column is its one-release shim (#NEW-14): every writer sets both, readers
// still read the column until it goes.
import { sql, type SQL } from "drizzle-orm"
import { repositories } from "@/db/schema"

/** For an UPDATE: the row's provider_config with `installationId` set. */
export function providerConfigWithInstallation(installationId: number | null): SQL {
  return sql`coalesce(${repositories.providerConfig}, '{}'::jsonb) || jsonb_build_object('installationId', ${installationId}::bigint)`
}

/** For an INSERT: a fresh GitHub provider_config. */
export function githubProviderConfig(installationId: number | null): { installationId: number | null } {
  return { installationId }
}
