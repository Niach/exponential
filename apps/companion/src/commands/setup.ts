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
  // WhatsApp pairing + per-chat target are handled from the web UI later;
  // repo mappings are deferred to the GitHub App follow-up. The companion is
  // safe to run with `projects = {}` — issues for unmapped projects will be
  // marked `needs_human`.
  const config: CompanionConfig = configSchema.parse({
    exponential: {
      baseUrl: opts.server,
      workspaceId: claimed.workspace.id,
      workspaceSlug: claimed.workspace.slug,
      agentId: claimed.agent.id,
      botUserId: claimed.agent.userId,
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
    messaging: {
      whatsapp: {
        enabled: true,
        authStateDir: join(STATE_DIR, `baileys-auth`),
      },
    },
    projects: {},
  })

  await saveConfig(config)
  await writeBotToken(claimed.apiKey)

  console.log(``)
  console.log(`Setup complete for workspace "${claimed.workspace.name}".`)
  console.log(`Config:    ${CONFIG_DIR}/config.toml`)
  console.log(`Token:     ${CONFIG_DIR}/bot.token`)
  console.log(``)
  console.log(`Next: pair WhatsApp from the workspace settings page.`)
}
