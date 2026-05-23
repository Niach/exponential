import { join } from "node:path"
import {
  CONFIG_DIR,
  STATE_DIR,
  configSchema,
  saveConfig,
  type CompanionConfig,
} from "../config"
import { writeBotToken } from "../credentials"
import { claimSetup } from "../exponential-api"

interface Opts {
  server: string
  setupToken: string
  driver?: `claude` | `codex`
}

export async function runSetup(opts: Opts): Promise<void> {
  if (!opts.server) throw new Error(`Missing --server`)
  if (!opts.setupToken) throw new Error(`Missing --setup-token`)

  const claimed = await claimSetup({
    baseUrl: opts.server,
    setupToken: opts.setupToken,
  })

  // Non-interactive: every value is either a CLI flag or a sensible default.
  // GitHub login is a separate post-setup step.
  const config: CompanionConfig = configSchema.parse({
    exponential: {
      baseUrl: opts.server,
      workspaceId: claimed.workspace.id,
      workspaceSlug: claimed.workspace.slug,
      agentId: claimed.agent.id,
      botUserId: claimed.agent.userId,
      githubOauthClientId: claimed.oauth?.githubClientId ?? undefined,
    },
    driver: {
      default: opts.driver ?? `claude`,
      maxConcurrentIssues: 2,
      turnTimeoutMs: 30 * 60_000,
      issueBudgetMs: 4 * 60 * 60_000,
    },
    worktrees: {
      root: join(STATE_DIR, `worktrees`),
      minFreeBytes: 5 * 1024 * 1024 * 1024,
      branchPrefix: `agent`,
    },
  })

  await saveConfig(config)
  await writeBotToken(claimed.apiKey)

  console.log(``)
  console.log(`Setup complete for workspace "${claimed.workspace.name}".`)
  console.log(`Config:    ${CONFIG_DIR}/config.toml`)
  console.log(`Token:     ${CONFIG_DIR}/bot.token`)
  console.log(``)
  console.log(`Next steps:`)
  console.log(`  1. Run \`bun apps/companion/src/cli.ts github login\` to`)
  console.log(`     authorize the companion against GitHub.`)
  console.log(`  2. Link each project to a repo in the workspace settings.`)
}
