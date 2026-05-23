import { loadConfig } from "./config"
import { openState } from "./state"
import { createLogger } from "./logger"
import { startEventSource } from "./event-source"
import { startDispatcher } from "./dispatcher"
import { buildIssuePipeline } from "./pipeline"
import {
  heartbeat,
  pollControl,
  reportGithubIdentity,
} from "./exponential-api"
import { loadAccessToken } from "./github-auth"
import { getAuthedUser, listAccessibleRepos } from "./github-api"
import { startPrPollLoop } from "./pr-poll-loop"
import type { CompanionConfig } from "./config"
import type { Logger } from "./logger"
import type { Dispatcher } from "./dispatcher"
import type { StateHandle } from "./state"

const ACTIVITY_CURSOR_KEY = `pollControl.activityCursor`

function startGithubIdentityLoop(config: CompanionConfig, log: Logger) {
  let stopped = false
  const tick = async () => {
    if (stopped) return
    const auth = await loadAccessToken().catch(() => null)
    if (!auth) return
    try {
      const user = await getAuthedUser(auth.token)
      const repos = await listAccessibleRepos(auth.token)
      await reportGithubIdentity(config, user.login, repos)
      log.debug(
        { login: user.login, repos: repos.length },
        `github identity refreshed`
      )
    } catch (e) {
      log.warn(
        { err: e instanceof Error ? e.message : String(e) },
        `github identity refresh failed`
      )
    }
  }
  void tick()
  const timer = setInterval(() => void tick(), 5 * 60_000)
  return () => {
    stopped = true
    clearInterval(timer)
  }
}

function startHeartbeatLoop(config: CompanionConfig, log: Logger) {
  let stopped = false
  const tick = async () => {
    if (stopped) return
    await heartbeat(config).catch((e) =>
      log.warn(
        { err: e instanceof Error ? e.message : String(e) },
        `heartbeat failed`
      )
    )
  }
  void tick()
  const timer = setInterval(() => void tick(), 30_000)
  return () => {
    stopped = true
    clearInterval(timer)
  }
}

// Polls the server for issues whose updated_at has advanced past our cursor —
// the fallback path for when the Electric ShapeStream isn't delivering live
// events (e.g., a reverse proxy stripping long-poll headers). Each issue is
// re-emitted as an `updated` dispatcher event; the REENTRY gate handles dedup.
function startActivityPollLoop(
  config: CompanionConfig,
  log: Logger,
  state: StateHandle,
  dispatcher: Dispatcher
) {
  let stopped = false

  const tick = async () => {
    if (stopped) return
    const activityCursor = state.kvGet(ACTIVITY_CURSOR_KEY)
    const control = await pollControl(config, { activityCursor }).catch((e) => {
      log.warn(
        { err: e instanceof Error ? e.message : String(e) },
        `control poll failed`
      )
      return null
    })
    if (!control) return

    for (const issue of control.activity.issues) {
      dispatcher.enqueue({
        type: `updated`,
        issueId: issue.id,
        identifier: issue.identifier,
        title: issue.title,
        projectId: issue.projectId,
        assigneeId: issue.assigneeId,
      })
    }
    if (control.activity.issues.length > 0) {
      log.info(
        {
          count: control.activity.issues.length,
          ids: control.activity.issues.map((i) => i.identifier),
        },
        `poll-control activity → dispatcher`
      )
    }
    state.kvSet(ACTIVITY_CURSOR_KEY, control.activity.cursor)
  }

  void tick()
  const timer = setInterval(() => void tick(), 5_000)
  return () => {
    stopped = true
    clearInterval(timer)
  }
}

export async function runDaemon() {
  const config = await loadConfig()
  const log = createLogger(config)
  const state = openState()

  log.info({ baseUrl: config.exponential.baseUrl }, `daemon starting`)

  const dispatcher = startDispatcher({
    config,
    state,
    log,
    pipeline: buildIssuePipeline(),
  })
  const eventSource = await startEventSource({ config, state, log, dispatcher })
  const stopHeartbeat = startHeartbeatLoop(config, log)
  const stopActivityPoll = startActivityPollLoop(config, log, state, dispatcher)
  const stopGithubIdentity = startGithubIdentityLoop(config, log)
  const prPoll = startPrPollLoop({ config, state, log })

  const shutdown = async (signal: string) => {
    log.info({ signal }, `shutting down`)
    stopHeartbeat()
    stopActivityPoll()
    stopGithubIdentity()
    prPoll.stop()
    await eventSource.stop()
    await dispatcher.stop()
    state.close()
    process.exit(0)
  }
  process.on(`SIGINT`, () => void shutdown(`SIGINT`))
  process.on(`SIGTERM`, () => void shutdown(`SIGTERM`))
}
