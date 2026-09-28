// EXP-1121: whether an issue can Start coding RIGHT NOW, as three ordered
// steps every client derives from data it already has — GitHub connected,
// a repository on the board, a device online. Start coding always renders
// for a member (dashed amber while a step is missing, the caption naming
// the FIRST missing one); a click opens the "Ready to code?" checklist, each
// unmet row carrying the fix. Pure and FIXTURE-LOCKED ×4
// (`packages/domain-contract/fixtures/coding-readiness.json`): desktop
// `domain::coding_readiness`, iOS `CodingReadiness.swift`, Android
// `CodingReadiness.kt` return byte-identical output, copy included.
//
// Remote start (the steer relay) is NOT a step: an instance without it has
// no remote coding at all, so the button stays hidden (`visible: false`).

export type ReadinessStepKey = `github` | `repository` | `device`

/** `met` = green tick; `current` = the FIRST unmet step (highlighted, fixes
 * shown); `pending` = unmet behind it (no fixes until it is current). */
export type ReadinessStepState = `met` | `current` | `pending`

export type ReadinessFix =
  | `connect_github`
  | `choose_repository`
  | `board_settings`
  | `open_devices`
  | `get_desktop_app`
  | `set_up_server`

export interface ReadinessDevice {
  label: string
  /** Registered by the caller (a teammate's shared server is not). */
  own: boolean
  online: boolean
  /** Unix ms, null when never seen. */
  lastSeenAtMs: number | null
}

export interface CodingReadinessInput {
  isMember: boolean
  /** `steer.config.enabled`; null while loading. */
  remoteStartEnabled: boolean | null
  teamName: string
  boardName: string
  /** The board's repository full name (`owner/name`), null = none. A board
   * with a `repositoryId` whose row has not resolved yet passes `""`. */
  boardRepository: string | null
  /** Asked only while the board has no repository: does the team have a
   * GitHub installation or repository? null = still loading. `label` = the
   * connected account (`acme-inc`) for the met row's detail. */
  github: { connected: boolean; label: string | null } | null
  /** Own devices + servers shared with the team; null while loading. */
  devices: ReadinessDevice[] | null
  nowMs: number
}

export interface ReadinessStep {
  key: ReadinessStepKey
  state: ReadinessStepState
  title: string
  body: string | null
  /** Right-aligned detail of a met row (account, repo, device label). */
  detail: string | null
  fixes: ReadinessFix[]
}

export interface CodingReadiness {
  /** Non-member or no remote start on this instance: render nothing. */
  visible: boolean
  /** Inputs still loading: the pill shows but stays inert, no caption. */
  loading: boolean
  ready: boolean
  metCount: number
  total: number
  steps: ReadinessStep[]
  /** "1 of 3 set up. Fix the rest here." */
  summary: string
  /** The pill's one-line caption: the first missing step, null when ready
   * or loading. */
  caption: string | null
}

// ── Copy (byte-identical ×4) ────────────────────────────────────────────
export const READINESS_COPY = {
  title: `Ready to code?`,
  start: `Start coding`,
  close: `Close`,
  captionGithub: `Needs GitHub`,
  captionRepository: `Needs a repository`,
  captionDevice: `No device online`,
  githubMet: `GitHub connected`,
  githubUnmet: `Connect GitHub`,
  githubBody: `Start coding clones a repository from a GitHub account or organization connected to the team.`,
  repositoryMet: `Repository connected`,
  deviceMet: `Device online`,
  deviceUnmet: `A device online`,
  deviceNeverBody: `Coding runs on the desktop app or on a server running the CLI. You haven’t set one up yet.`,
  fixConnectGithub: `Connect GitHub`,
  fixChooseRepository: `Choose repository`,
  fixBoardSettings: `Board settings`,
  fixOpenDevices: `Open Devices`,
  fixGetDesktopApp: `Get the desktop app`,
  fixSetUpServer: `Set up a server`,
  pickerSearch: `Search repositories…`,
  pickerMatchesBoard: `matches board`,
  pickerAddFromGithub: `Add another repository from GitHub…`,
  pickerEmpty: `No repositories connected to the team yet.`,
  allSet: `All set.`,
  oneLeft: `One step left.`,
  fixRest: `Fix the rest here.`,
  justNow: `just now`,
} as const

export const readinessRepositoryTitle = (board: string) =>
  `Connect a repository to ${board}`
export const readinessRepositoryBody = (board: string) =>
  `Start coding clones the board’s repository. ${board} has none yet.`
export const readinessDeviceBody = (team: string) =>
  `None of your devices, or the ones shared with ${team}, is online.`
export const readinessLastSeen = (label: string, ago: string) =>
  `Your ${label} was last seen ${ago}.`
export const readinessPickerUsedBy = (board: string) => `used by ${board}`
export const readinessSummary = (met: number, total: number) => {
  const tail =
    met === total
      ? READINESS_COPY.allSet
      : total - met === 1
        ? READINESS_COPY.oneLeft
        : READINESS_COPY.fixRest
  return `${met} of ${total} set up. ${tail}`
}

/** "just now" / "5 min ago" / "2 h ago" / "3 d ago" (floored). */
export function readinessAgo(nowMs: number, thenMs: number): string {
  const seconds = Math.max(0, Math.floor((nowMs - thenMs) / 1000))
  if (seconds < 60) return READINESS_COPY.justNow
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} h ago`
  return `${Math.floor(hours / 24)} d ago`
}

const CAPTIONS: Record<ReadinessStepKey, string> = {
  github: READINESS_COPY.captionGithub,
  repository: READINESS_COPY.captionRepository,
  device: READINESS_COPY.captionDevice,
}

/** The caller's most recently seen OWN device, for "Your … was last seen"
 * (a teammate's shared server is never "yours"). First wins a tie. */
function lastSeenDevice(devices: ReadinessDevice[]): ReadinessDevice | null {
  let best: ReadinessDevice | null = null
  for (const device of devices) {
    if (!device.own || device.lastSeenAtMs == null) continue
    if (!best || device.lastSeenAtMs > (best.lastSeenAtMs ?? 0)) best = device
  }
  return best
}

export function codingReadiness(input: CodingReadinessInput): CodingReadiness {
  const hasRepository = input.boardRepository !== null
  const visible = input.isMember && input.remoteStartEnabled !== false
  const loading =
    input.remoteStartEnabled === null ||
    input.devices === null ||
    (!hasRepository && input.github === null)

  const devices = input.devices ?? []
  const online = devices.find((device) => device.online) ?? null
  const githubMet = hasRepository || input.github?.connected === true
  const met: Record<ReadinessStepKey, boolean> = {
    github: githubMet,
    repository: hasRepository,
    device: online !== null,
  }
  const order: ReadinessStepKey[] = [`github`, `repository`, `device`]
  const firstMissing = order.find((key) => !met[key]) ?? null
  const state = (key: ReadinessStepKey): ReadinessStepState =>
    met[key] ? `met` : key === firstMissing ? `current` : `pending`

  const seen = lastSeenDevice(devices)
  const lastSeenLine =
    seen && seen.lastSeenAtMs != null
      ? readinessLastSeen(seen.label, readinessAgo(input.nowMs, seen.lastSeenAtMs))
      : null
  const registered = devices.some((device) => device.own)

  const steps: ReadinessStep[] = order.map((key) => {
    const s = state(key)
    if (key === `github`) {
      return s === `met`
        ? {
            key,
            state: s,
            title: READINESS_COPY.githubMet,
            body: null,
            detail: input.github?.label ?? null,
            fixes: [],
          }
        : {
            key,
            state: s,
            title: READINESS_COPY.githubUnmet,
            body: READINESS_COPY.githubBody,
            detail: null,
            fixes: [`connect_github`],
          }
    }
    if (key === `repository`) {
      return s === `met`
        ? {
            key,
            state: s,
            title: READINESS_COPY.repositoryMet,
            body: null,
            detail: input.boardRepository || null,
            fixes: [],
          }
        : {
            key,
            state: s,
            title: readinessRepositoryTitle(input.boardName),
            body: readinessRepositoryBody(input.boardName),
            detail: null,
            fixes: s === `current` ? [`choose_repository`, `board_settings`] : [],
          }
    }
    if (s === `met`) {
      return {
        key,
        state: s,
        title: READINESS_COPY.deviceMet,
        body: null,
        detail: online?.label ?? null,
        fixes: [],
      }
    }
    if (s === `pending`) {
      return {
        key,
        state: s,
        title: READINESS_COPY.deviceUnmet,
        body: lastSeenLine,
        detail: null,
        fixes: [],
      }
    }
    const body = !registered
      ? READINESS_COPY.deviceNeverBody
      : [readinessDeviceBody(input.teamName), lastSeenLine]
          .filter(Boolean)
          .join(` `)
    return {
      key,
      state: s,
      title: READINESS_COPY.deviceUnmet,
      body,
      detail: null,
      fixes: registered
        ? [`open_devices`, `get_desktop_app`, `set_up_server`]
        : [`get_desktop_app`, `set_up_server`],
    }
  })

  const metCount = order.filter((key) => met[key]).length
  return {
    visible,
    loading,
    ready: !loading && firstMissing === null,
    metCount,
    total: order.length,
    steps,
    summary: readinessSummary(metCount, order.length),
    caption:
      !visible || loading || firstMissing === null ? null : CAPTIONS[firstMissing],
  }
}
