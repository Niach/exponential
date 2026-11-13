// #EXP-1614: `repositories.provider_config` (server-only) holds the
// provider's settings; GitHub's is `{installationId}`. It replaced the
// `installation_id` column (dropped by contract step
// `0274_drop_repositories_installation_id`).
import { sql, type SQL } from "drizzle-orm"
import { repositories } from "@/db/schema"

/** The row's GitHub installation, as a selectable/comparable column. */
export function repoInstallationId(): SQL<number | null> {
  return sql<number | null>`(${repositories.providerConfig}->>'installationId')::bigint`.mapWith(Number)
}

/** The installation id out of a loaded row's provider_config. */
export function installationIdOf(config: unknown): number | null {
  const id = (config as { installationId?: unknown } | null | undefined)?.installationId
  return typeof id === `number` ? id : typeof id === `string` && id ? Number(id) : null
}

/** For an UPDATE: the row's provider_config with `installationId` set. */
export function providerConfigWithInstallation(installationId: number | null): SQL {
  return sql`coalesce(${repositories.providerConfig}, '{}'::jsonb) || jsonb_build_object('installationId', ${installationId}::bigint)`
}

/** For an INSERT: a fresh GitHub provider_config. */
export function githubProviderConfig(installationId: number | null): { installationId: number | null } {
  return { installationId }
}
