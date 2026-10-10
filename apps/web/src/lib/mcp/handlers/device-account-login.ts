// EXP-1199: `exponential_devices_account_login`: sign an agent account in on
// one of the caller's OWN machines from MCP, the same `agent_login` →
// `agent_login_code` device commands every client's "Add account" / "Sign in"
// queues (`hooks/use-agent-login.ts`, iOS `AgentLoginSheet`, Android
// `AgentsScreen`, desktop `agent_login.rs`). The machine runs the agent CLI's
// own login in a fresh staging dir and completes the command EARLY with the
// sign-in URL (+ codex's device code); claude's browser code comes back as
// `agent_login_code`. The login then lands on the profile whose EMAIL it
// signed in as (an address the machine already holds is refreshed, a new one
// becomes a profile). Only the URL and the typed code travel: the credential
// is written by the CLI on the machine and never leaves it.
//
// One call per step, each bounded: the tool waits a little for the device's
// answer and otherwise hands back the `commandId` to check on.
import { parseAgentLoginResult } from "@/lib/agent-usage"
import type { AgentLoginProfileTarget } from "@/hooks/use-agent-login"

export interface DeviceAccountLoginInput {
  deviceId: string
  agent: string
  /** The login this sign-in is FOR (a re-login); absent = add an account. */
  profileId?: string
  code?: string
  commandId?: string
}

export interface DeviceCommandRow {
  id: string
  kind: string
  status: string
  result: string | null
  payload?: Record<string, string> | null
}

export interface DeviceAccountLoginDeps {
  /** `devices.createCommand` — refuses a device that is not the caller's. */
  createCommand: (
    input:
      | ({ deviceId: string; kind: `agent_login`; agent: string; switch: false } & AgentLoginProfileTarget)
      | { deviceId: string; kind: `agent_login_code`; agent: string; code: string }
  ) => Promise<{ id: string }>
  getCommand: (commandId: string) => Promise<DeviceCommandRow>
  sleep?: (ms: number) => Promise<void>
}

export type DeviceAccountLoginResult =
  | {
      status: `pending`
      commandId: string
      next: string
    }
  | {
      status: `url`
      commandId: string
      url: string
      code: string | null
      profileId: string | null
      next: string
    }
  | { status: `signing_in`; commandId: string; next: string }
  | { status: `failed`; commandId: string; message: string }

/** How long one call waits for the machine (the relay nudge answers an online
 *  one in ~1-2s; an offline one keeps the command queued). */
export const LOGIN_WAIT_MS = 25_000
export const CODE_WAIT_MS = 15_000
const POLL_MS = 1_000

const PENDING_NEXT = `The machine has not answered yet (offline or busy); call again with this commandId.`
const SIGNING_IN_NEXT = `The code went in. The login appears under agentAccounts in exponential_devices_list after the machine's next heartbeat (~30s).`

function urlNext(code: string | null): string {
  return code
    ? `Open url in a browser and enter code there; the login then lands on the machine by itself.`
    : `Open url in a browser and sign in; then call again with the code the browser shows (code).`
}

function describe(command: DeviceCommandRow): DeviceAccountLoginResult {
  if (command.status === `pending`) {
    return { status: `pending`, commandId: command.id, next: PENDING_NEXT }
  }
  if (command.status === `failed`) {
    return {
      status: `failed`,
      commandId: command.id,
      message: command.result ?? `The device reported a failure.`,
    }
  }
  if (command.kind === `agent_login_code`) {
    return { status: `signing_in`, commandId: command.id, next: SIGNING_IN_NEXT }
  }
  const progress = parseAgentLoginResult(command.result)
  if (!progress || progress.phase === `failed` || !progress.url) {
    return {
      status: `failed`,
      commandId: command.id,
      message: progress?.message ?? command.result ?? `The device reported a failure.`,
    }
  }
  return {
    status: `url`,
    commandId: command.id,
    url: progress.url,
    code: progress.code,
    profileId: profileIdOf(command.result),
    next: urlNext(progress.code),
  }
}

/** `LoginProgress.profileId`, when the machine names one (a login lands by
 *  email, so its profile is often only known after it commits). */
function profileIdOf(result: string | null): string | null {
  if (!result) return null
  try {
    const value = JSON.parse(result) as { profileId?: unknown }
    return typeof value.profileId === `string` && value.profileId ? value.profileId : null
  } catch {
    return null
  }
}

async function waitFor(
  commandId: string,
  deps: DeviceAccountLoginDeps,
  budgetMs: number
): Promise<DeviceCommandRow> {
  const sleep = deps.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms)))
  let waited = 0
  for (;;) {
    const command = await deps.getCommand(commandId)
    if (command.status !== `pending` || waited >= budgetMs) return command
    await sleep(POLL_MS)
    waited += POLL_MS
  }
}

export async function deviceAccountLogin(
  input: DeviceAccountLoginInput,
  deps: DeviceAccountLoginDeps
): Promise<DeviceAccountLoginResult> {
  const { deviceId, agent } = input
  if (input.commandId) {
    const command = await deps.getCommand(input.commandId)
    const budget = command.kind === `agent_login_code` ? CODE_WAIT_MS : LOGIN_WAIT_MS
    return describe(await waitFor(command.id, deps, budget))
  }

  if (input.code) {
    const { id } = await deps.createCommand({
      deviceId,
      kind: `agent_login_code`,
      agent,
      code: input.code,
    })
    return describe(await waitFor(id, deps, CODE_WAIT_MS))
  }

  // The SAME command as every client's Add account / Sign in: no profile =
  // add, a profileId = the login this re-login is for.
  const { id } = await deps.createCommand({
    deviceId,
    kind: `agent_login`,
    agent,
    switch: false,
    ...(input.profileId ? { profileId: input.profileId } : {}),
  })
  return describe(await waitFor(id, deps, LOGIN_WAIT_MS))
}
