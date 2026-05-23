import type { CompanionConfig } from "./config"
import { readBotToken } from "./credentials"

interface TrpcResult<T> {
  result?: { data: T }
  error?: { message?: string }
}

export interface ClaimSetupResult {
  apiKey: string
  agent: {
    id: string
    userId: string
    name: string
  }
  workspace: {
    id: string
    slug: string
    name: string
  }
  projects: Array<{
    id: string
    name: string
    slug: string
    prefix: string
  }>
  oauth?: {
    githubClientId: string | null
  }
}

export interface PollActivityIssue {
  id: string
  identifier: string
  title: string
  projectId: string
  assigneeId: string | null
  updatedAt: string
}

export interface CompanionControl {
  whatsappPairingRequestedAt: string | Date | null
  whatsappStatus: string
  whatsappNotifyJid: string | null
  activity: {
    cursor: string
    issues: PollActivityIssue[]
  }
}

function endpoint(baseUrl: string, path: string): string {
  return `${baseUrl.replace(/\/$/, ``)}/api/trpc/${path}`
}

async function callTrpc<T>(
  baseUrl: string,
  path: string,
  input: unknown,
  token?: string
): Promise<T> {
  const res = await fetch(endpoint(baseUrl, path), {
    method: `POST`,
    headers: {
      "content-type": `application/json`,
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: input === undefined ? undefined : JSON.stringify(input),
  })
  const payload = (await res.json().catch(() => null)) as TrpcResult<T> | null
  if (!res.ok || payload?.error) {
    throw new Error(
      payload?.error?.message ??
        `Exponential API ${path} failed (${res.status})`
    )
  }
  if (!payload || !payload.result) {
    throw new Error(`Exponential API ${path} returned an invalid response`)
  }
  return payload.result.data
}

export async function claimSetup(args: {
  baseUrl: string
  setupToken: string
}): Promise<ClaimSetupResult> {
  return callTrpc<ClaimSetupResult>(args.baseUrl, `companion.claimSetup`, {
    setupToken: args.setupToken,
  })
}

export async function heartbeat(config: CompanionConfig): Promise<void> {
  const token = await readBotToken()
  await callTrpc(
    config.exponential.baseUrl,
    `companion.heartbeat`,
    undefined,
    token
  )
}

export async function pollControl(
  config: CompanionConfig,
  args?: { activityCursor?: string | null }
): Promise<CompanionControl> {
  const token = await readBotToken()
  const input = args?.activityCursor
    ? { activityCursor: args.activityCursor }
    : {}
  return callTrpc<CompanionControl>(
    config.exponential.baseUrl,
    `companion.pollControl`,
    input,
    token
  )
}

export async function reportWhatsappQr(
  config: CompanionConfig,
  qr: string
): Promise<void> {
  const token = await readBotToken()
  await callTrpc(
    config.exponential.baseUrl,
    `companion.reportWhatsappQr`,
    { qr },
    token
  )
}

export async function reportWhatsappStatus(
  config: CompanionConfig,
  status: `connected` | `disconnected` | `error`,
  error?: string | null
): Promise<void> {
  const token = await readBotToken()
  await callTrpc(
    config.exponential.baseUrl,
    `companion.reportWhatsappStatus`,
    { status, error: error ?? null },
    token
  )
}

export async function reportWhatsappOwnJid(
  config: CompanionConfig,
  jid: string
): Promise<void> {
  const token = await readBotToken()
  await callTrpc(
    config.exponential.baseUrl,
    `companion.reportWhatsappOwnJid`,
    { jid },
    token
  )
}

export async function reportWhatsappChats(
  config: CompanionConfig,
  chats: Array<{ jid: string; name: string; isGroup: boolean }>
): Promise<void> {
  const token = await readBotToken()
  await callTrpc(
    config.exponential.baseUrl,
    `companion.reportWhatsappChats`,
    { chats },
    token
  )
}

export async function uninstallSelf(config: CompanionConfig): Promise<void> {
  const token = await readBotToken()
  await callTrpc(
    config.exponential.baseUrl,
    `companion.uninstallSelf`,
    undefined,
    token
  )
}

export async function reportGithubIdentity(
  config: CompanionConfig,
  login: string,
  repos: Array<{ fullName: string; defaultBranch: string; private: boolean }>
): Promise<void> {
  const token = await readBotToken()
  await callTrpc(
    config.exponential.baseUrl,
    `companion.reportGithubIdentity`,
    { login, repos },
    token
  )
}

export async function clearGithubIdentity(
  config: CompanionConfig
): Promise<void> {
  const token = await readBotToken()
  await callTrpc(
    config.exponential.baseUrl,
    `companion.clearGithubIdentity`,
    undefined,
    token
  )
}
