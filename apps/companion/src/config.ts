import { homedir } from "node:os"
import { join } from "node:path"
import { readFile, mkdir, writeFile } from "node:fs/promises"
import TOML from "@iarna/toml"
import { z } from "zod"

export const CONFIG_DIR = join(homedir(), `.config`, `exponential-companion`)
export const STATE_DIR = join(homedir(), `.exponential-companion`)
export const CONFIG_PATH = join(CONFIG_DIR, `config.toml`)

export const configSchema = z.object({
  exponential: z.object({
    baseUrl: z.string().url(),
    workspaceId: z.string().uuid(),
    workspaceSlug: z.string().min(1),
    agentId: z.string().uuid(),
    botUserId: z.string().uuid(),
  }),
  driver: z.object({
    default: z.enum([`claude`, `codex`]).default(`claude`),
    maxConcurrentIssues: z.number().int().min(1).max(8).default(2),
    turnTimeoutMs: z
      .number()
      .int()
      .min(60_000)
      .default(30 * 60_000),
    issueBudgetMs: z
      .number()
      .int()
      .min(60_000)
      .default(4 * 60 * 60_000),
  }),
  worktrees: z.object({
    root: z.string().default(join(STATE_DIR, `worktrees`)),
    minFreeBytes: z
      .number()
      .int()
      .min(0)
      .default(5 * 1024 * 1024 * 1024),
    branchPrefix: z.string().default(`agent`),
  }),
  messaging: z
    .object({
      whatsapp: z
        .object({
          enabled: z.boolean(),
          notifyJid: z
            .string()
            .regex(/^\d+@s\.whatsapp\.net$/)
            .optional(),
          authStateDir: z.string(),
        })
        .optional(),
    })
    .optional(),
  projects: z
    .record(
      z.string(),
      z.object({
        repoPath: z.string(),
        defaultBranch: z.string().default(`main`),
        testCommand: z.string().optional(),
      })
    )
    .default({}),
})

export type CompanionConfig = z.infer<typeof configSchema>

export async function loadConfig(): Promise<CompanionConfig> {
  const raw = await readFile(CONFIG_PATH, `utf-8`)
  const parsed = TOML.parse(raw) as unknown
  return configSchema.parse(parsed)
}

export async function saveConfig(config: CompanionConfig): Promise<void> {
  await mkdir(CONFIG_DIR, { recursive: true })
  await mkdir(STATE_DIR, { recursive: true })
  const toml = TOML.stringify(config as unknown as TOML.JsonMap)
  await writeFile(CONFIG_PATH, toml, { mode: 0o600 })
}
