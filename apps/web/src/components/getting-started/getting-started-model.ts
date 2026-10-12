import spec from "@exp/domain-contract/fixtures/getting-started.json"

// EXP-1270 (EXP-1259 D6): the pure Getting started model, replayed ×4 off
// `packages/domain-contract/fixtures/getting-started.json` (desktop
// getting_started.rs, iOS GettingStartedProgress.swift, Android
// GettingStartedModel.kt) — no React. The hook
// (`use-getting-started-progress`) gathers the signals; this derives the ONE
// row of cards: eight steps in the fixture's order, the first open one NEXT,
// done ones keep their check and stay clickable, no locked state (a step
// needing a device opens the device step until there is one), owner-only
// steps hidden from members, `phones: false` steps hidden on the phones.

export type GettingStartedStepKey =
  | `board`
  | `github`
  | `device`
  | `run`
  | `invite`
  | `server`
  | `mcp`
  | `widget`

export type GettingStartedTarget =
  | `create-board`
  | `github`
  | `add-device`
  | `agent`
  | `invite`
  | `add-server`
  | `mcp`
  | `widget`

export interface GettingStartedSignals {
  /** Any live (non-trashed) board. */
  hasBoard: boolean
  /** The viewer's GitHub lists an installed account, or a board has a repo. */
  githubInstalled: boolean
  /** The caller owns a desktop device. */
  hasDesktopDevice: boolean
  /** The caller owns a server (CLI daemon) device. */
  hasServerDevice: boolean
  /** Any coding_sessions row in the team. */
  hasCodingSession: boolean
  /** More than one member, or an invite row. */
  hasInvitedTeam: boolean
  /** An MCP OAuth grant or a personal API key. */
  mcpConnected: boolean
  /** A feedback widget exists (owner-only signal). */
  hasWidget: boolean
}

export interface GettingStartedPermissions {
  canManageMembers: boolean
  canManageWidgets: boolean
}

export type GettingStartedPlatform = `pointer` | `phone`

export interface GettingStartedEntry {
  key: GettingStartedStepKey
  done: boolean
  /** What a click runs (`add-device` while a device-needing step has none). */
  target: GettingStartedTarget
  /** The first step not done: the highlighted card. */
  next: boolean
}

export interface GettingStartedState {
  entries: GettingStartedEntry[]
  done: number
  total: number
  /** `{done} of {total}`. */
  progress: string
  /** Every visible step is done: the block and the entry go away. */
  complete: boolean
}

export interface GettingStartedStep {
  key: GettingStartedStepKey
  title: string
  icon: string
  target: GettingStartedTarget
  needsDevice?: boolean
  visibleWhen?: keyof GettingStartedPermissions
  phones: boolean
  /** Signal names OR'd together (`hasDesktopDevice || hasServerDevice`). */
  doneWhen: string
}

export const GETTING_STARTED_STEPS = spec.steps as readonly GettingStartedStep[]

/** `doneWhen` is a disjunction of signal names: the fixture's own words. */
function isDone(step: GettingStartedStep, signals: GettingStartedSignals) {
  return step.doneWhen
    .split(`||`)
    .some((name) => signals[name.trim() as keyof GettingStartedSignals] === true)
}

export function formatGettingStartedProgress(done: number, total: number) {
  return spec.progress
    .replace(`{done}`, String(done))
    .replace(`{total}`, String(total))
}

export function deriveGettingStarted(
  signals: GettingStartedSignals,
  permissions: GettingStartedPermissions,
  platform: GettingStartedPlatform
): GettingStartedState {
  const hasDevice = signals.hasDesktopDevice || signals.hasServerDevice
  const visible = GETTING_STARTED_STEPS.filter(
    (step) =>
      (platform === `pointer` || step.phones) &&
      (step.visibleWhen === undefined || permissions[step.visibleWhen])
  )
  let nextTaken = false
  const entries = visible.map((step): GettingStartedEntry => {
    const done = isDone(step, signals)
    const next = !done && !nextTaken
    if (next) nextTaken = true
    return {
      key: step.key,
      done,
      target: step.needsDevice && !hasDevice ? `add-device` : step.target,
      next,
    }
  })
  const done = entries.filter((entry) => entry.done).length
  const total = entries.length
  return {
    entries,
    done,
    total,
    progress: formatGettingStartedProgress(done, total),
    complete: total > 0 && done === total,
  }
}
