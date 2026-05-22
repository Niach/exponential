import { createInterface } from "node:readline/promises"
import { stdin as input, stdout as output } from "node:process"
import { stat } from "node:fs/promises"
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
}

async function pathExists(path: string): Promise<boolean> {
  return stat(path)
    .then(() => true)
    .catch(() => false)
}

export async function runSetup(opts: Opts): Promise<void> {
  if (!opts.server) throw new Error(`Missing --server`)
  if (!opts.setupToken) throw new Error(`Missing --setup-token`)

  const claimed = await claimSetup({
    baseUrl: opts.server,
    setupToken: opts.setupToken,
  })

  const rl = createInterface({ input, output })
  const ask = async (q: string, def?: string): Promise<string> => {
    const suffix = def ? ` [${def}]` : ``
    const answer = (await rl.question(`${q}${suffix}: `)).trim()
    return answer || def || ``
  }

  console.log(`Setting up Exponential Agent Companion.`)
  console.log(`Workspace: ${claimed.workspace.name}`)
  console.log(`Config will be written to ${CONFIG_DIR}/config.toml`)
  console.log()

  const driverDefault = (await ask(
    `Default driver (claude or codex)`,
    `claude`
  )) as `claude` | `codex`
  if (driverDefault !== `claude` && driverDefault !== `codex`) {
    rl.close()
    throw new Error(`driver must be 'claude' or 'codex'`)
  }

  const phone = await ask(
    `WhatsApp notification phone, digits only (blank to skip)`,
    ``
  )
  let notifyJid: string | undefined
  if (phone) {
    if (!/^\d+$/.test(phone)) {
      rl.close()
      throw new Error(`Phone must be digits only, no + or spaces`)
    }
    notifyJid = `${phone}@s.whatsapp.net`
  }

  const projects: CompanionConfig[`projects`] = {}
  for (const project of claimed.projects) {
    console.log(``)
    console.log(`Project ${project.name} (${project.prefix})`)
    const repoPath = await ask(`Local repo path (blank to skip)`, ``)
    if (!repoPath) continue
    if (!(await pathExists(repoPath))) {
      rl.close()
      throw new Error(`Repo path does not exist: ${repoPath}`)
    }
    const defaultBranch = await ask(`Default branch`, `main`)
    const testCommand = await ask(`Test command (blank to skip)`, ``)
    projects[project.id] = {
      repoPath,
      defaultBranch,
      ...(testCommand ? { testCommand } : {}),
    }
  }

  rl.close()

  const config: CompanionConfig = configSchema.parse({
    exponential: {
      baseUrl: opts.server,
      workspaceId: claimed.workspace.id,
      workspaceSlug: claimed.workspace.slug,
      agentId: claimed.agent.id,
      botUserId: claimed.agent.userId,
    },
    driver: {
      default: driverDefault,
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
        notifyJid,
        authStateDir: join(STATE_DIR, `baileys-auth`),
      },
    },
    projects,
  })

  await saveConfig(config)
  await writeBotToken(claimed.apiKey)

  console.log(``)
  console.log(`Config written to ${CONFIG_DIR}/config.toml`)
  console.log(`Bot token written to ${CONFIG_DIR}/bot.token`)
}
