// #EXP-1617 (Q6): team runner devices ALSO serve GitHub Actions as
// self-hosted runners. A repository opts in (`ci_config.actionsRunners`); then
// every enabled runner device of its team (`devices.runner.enabled`, cap
// `actions-runner`) registers one official `actions/runner` for it, labelled
// `self-hosted` + its OS + its runner labels (`exp-linux` / `exp-mac` are
// device labels, so `pick-runner.yml` keeps finding them). The daemon asks
// `ci.actionsRunners` what to serve and `ci.actionsRunnerToken` for each
// registration (ephemeral: one job, then a fresh registration) or removal.
//
// Tokens come from ONE seam (`RunnerTokenSource`): the GitHub App's
// installation token today (it needs Administration: write on the repo, a
// grant a person accepts on github.com; without it the error says so); a
// team-provided PAT sealed in the vault can slot in behind the same seam.

import { and, eq, inArray, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { db } from "@/db/connection"
import { devices, repositories, teamMembers, type DeviceRunnerSettings, type RepositoryCiConfig } from "@/db/schema"
import { resolveRepoInstallationToken, githubApiHeaders } from "@/lib/integrations/github-app"
import { githubFetch } from "@/lib/integrations/github-client"
import { githubErrorMessage } from "@/lib/integrations/github-error"
import { runnerLabels } from "@/lib/ci/state"

/** The daemon cap of a build that runs `actions/runner` for its team. */
export const ACTIONS_RUNNER_CAP = `actions-runner`

export type RunnerTokenKind = `registration` | `remove`

export interface RunnerToken {
  token: string
  /** ISO time GitHub lets it expire (about an hour). */
  expiresAt: string | null
}

/** Where runner tokens come from (the App today; a team PAT later). */
export interface RunnerTokenSource {
  mint(repo: { fullName: string; installationId: number | null }, kind: RunnerTokenKind): Promise<RunnerToken>
}

export const ADMINISTRATION_WRITE_MESSAGE = (fullName: string) =>
  `The GitHub App cannot register self-hosted runners on ${fullName}: it needs the Administration: write permission (accept it on github.com, in the App installation's settings).`

/** The App's installation token → `POST …/actions/runners/{registration,remove}-token`. */
export const appTokenSource: RunnerTokenSource = {
  async mint(repo, kind) {
    const installationToken = await resolveRepoInstallationToken(repo.fullName, { fallbackInstallationId: repo.installationId })
    if (!installationToken) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The GitHub App has no access to ${repo.fullName}` })
    const res = await githubFetch(`/repos/${repo.fullName}/actions/runners/${kind}-token`, {
      method: `POST`,
      headers: githubApiHeaders(installationToken),
    })
    if (res.status === 403 || res.status === 404) {
      throw new TRPCError({ code: `PRECONDITION_FAILED`, message: ADMINISTRATION_WRITE_MESSAGE(repo.fullName) })
    }
    if (!res.ok) throw new Error(`GitHub returned ${res.status} minting a runner ${kind} token for ${repo.fullName}: ${await githubErrorMessage(res)}`)
    const data = (await res.json()) as { token?: string; expires_at?: string }
    if (!data.token) throw new Error(`GitHub returned no runner ${kind} token for ${repo.fullName}`)
    return { token: data.token, expiresAt: data.expires_at ?? null }
  },
}

let tokenSource: RunnerTokenSource = appTokenSource

/** Tests (and a PAT source later): swap where tokens come from. */
export function setRunnerTokenSourceForTests(next: RunnerTokenSource | null): void {
  tokenSource = next ?? appTokenSource
}

export interface ActionsRunnerTarget {
  repositoryId: string
  fullName: string
  /** What `config.sh --url` takes. */
  url: string
  /** `config.sh --labels` (self-hosted + OS + the device's labels). */
  labels: string[]
  /** `config.sh --name`: unique per device and repo. */
  name: string
}

type DeviceRow = {
  id: string
  userId: string
  label: string
  platform: string | null
  caps: string[]
  runner: DeviceRunnerSettings | null
}

async function ownDevice(userId: string, deviceId: string): Promise<DeviceRow> {
  const [device] = (await db
    .select({ id: devices.id, userId: devices.userId, label: devices.label, platform: devices.platform, caps: devices.caps, runner: devices.runner })
    .from(devices)
    .where(and(eq(devices.userId, userId), eq(devices.deviceId, deviceId)))
    .limit(1)) as DeviceRow[]
  if (!device) throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
  return device
}

const slug = (text: string) => text.toLowerCase().replace(/[^a-z0-9-]+/g, `-`).replace(/^-+|-+$/g, ``).slice(0, 40) || `device`

/** The GitHub repositories this device serves Actions for (empty while its
 *  runner is off or its build cannot). */
export async function actionsRunnerTargets(userId: string, deviceId: string): Promise<ActionsRunnerTarget[]> {
  const device = await ownDevice(userId, deviceId)
  if (!device.runner?.enabled || !device.caps.includes(ACTIONS_RUNNER_CAP)) return []
  const teamIds = db.select({ id: teamMembers.teamId }).from(teamMembers).where(eq(teamMembers.userId, userId))
  const repos = await db
    .select({ id: repositories.id, fullName: repositories.fullName })
    .from(repositories)
    .where(
      and(
        inArray(repositories.teamId, teamIds),
        eq(repositories.provider, `github`),
        sql`(${repositories.ciConfig}->>'actionsRunners')::boolean IS TRUE`,
        sql`${repositories.archivedAt} IS NULL`
      )
    )
  const labels = [`self-hosted`, ...runnerLabels(device.platform, device.runner.labels)]
  // One registration per (device, full name): two teams connecting one repo
  // still give the machine one runner there.
  const seen = new Set<string>()
  return repos
    .filter((repo) => !seen.has(repo.fullName) && seen.add(repo.fullName))
    .sort((a, b) => (a.fullName < b.fullName ? -1 : a.fullName > b.fullName ? 1 : 0))
    .map((repo) => ({
      repositoryId: repo.id,
      fullName: repo.fullName,
      url: `https://github.com/${repo.fullName}`,
      labels,
      name: `${slug(device.label)}-${slug(repo.fullName.split(`/`).pop() ?? repo.fullName)}`,
    }))
}

/** A registration token for one of this device's targets, or a removal token
 *  for any repo of its teams (deregistering after the switch went off). */
export async function actionsRunnerToken(args: {
  userId: string
  deviceId: string
  repositoryId: string
  kind: RunnerTokenKind
}): Promise<RunnerToken> {
  const device = await ownDevice(args.userId, args.deviceId)
  const [repo] = await db
    .select({ id: repositories.id, teamId: repositories.teamId, fullName: repositories.fullName, provider: repositories.provider, providerConfig: repositories.providerConfig, ciConfig: repositories.ciConfig })
    .from(repositories)
    .where(eq(repositories.id, args.repositoryId))
    .limit(1)
  if (!repo || repo.provider !== `github`) throw new TRPCError({ code: `NOT_FOUND`, message: `Repository not found` })
  const [member] = await db
    .select({ id: teamMembers.id })
    .from(teamMembers)
    .where(and(eq(teamMembers.teamId, repo.teamId), eq(teamMembers.userId, device.userId)))
    .limit(1)
  if (!member) throw new TRPCError({ code: `FORBIDDEN`, message: `Not a member of this repository's team` })
  if (args.kind === `registration`) {
    const serves = device.runner?.enabled && device.caps.includes(ACTIONS_RUNNER_CAP) && (repo.ciConfig as RepositoryCiConfig).actionsRunners === true
    if (!serves) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `This device does not serve GitHub Actions for ${repo.fullName}` })
  }
  const installationId = (repo.providerConfig as { installationId?: number | null }).installationId ?? null
  return tokenSource.mint({ fullName: repo.fullName, installationId }, args.kind)
}
