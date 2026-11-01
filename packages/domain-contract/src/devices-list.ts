// A-16 / EXP-1560: the Devices page card — which group a device sits in,
// whether it says "No account", which logins wear the attention badge (and
// what their preview says), the update line its ⋯ menu leads with, and how
// many account icons show. Pure and byte-locked by
// `fixtures/devices-list.json` (`cases`), which every mirror replays: desktop
// `ui::machines` (`device_attention`), iOS `ExpCore/Sources/Domain/DevicesList.swift`,
// Android `domain/DevicesList.kt`. The fixture's `$comment` holds the rules;
// this is the reference.

import fixture from "../fixtures/devices-list.json" with { type: "json" }

export const DEVICES_LIST_COPY = fixture.copy
/** EXP-1560: the card grid's geometry (`fixture.card`), ×4. */
export const DEVICE_CARD = fixture.card
/** EXP-1600: the Keep signed in reminder window (`fixture.keepSignedIn`). */
export const KEEP_SIGNED_IN = fixture.keepSignedIn

const DAY_MS = 24 * 60 * 60 * 1000

/** EXP-1600: a long-lived claude login that is due for renewal at `now`:
 *  its token ends within `renewWithinDays` days, or already ended. */
export function keepSignedInDue(tokenExpiresAt: string, now: string | Date): boolean {
  const expires = Date.parse(tokenExpiresAt)
  const at = typeof now === `string` ? Date.parse(now) : now.getTime()
  if (Number.isNaN(expires) || Number.isNaN(at)) return true
  return expires - at <= KEEP_SIGNED_IN.renewWithinDays * DAY_MS
}

/** EXP-1560: grid columns for a section `width` px wide. */
export function deviceCardColumns(width: number): number {
  return Math.max(1, Math.floor((width + DEVICE_CARD.gap) / (DEVICE_CARD.minWidth + DEVICE_CARD.gap)))
}

/** EXP-1560: how many account icons a card shows, and whether the ⋯
 *  "More accounts" button follows them (`fixture.accountIcons`). */
export function deviceCardAccountIcons(
  count: number,
  expanded: boolean
): { visible: number; more: boolean } {
  if (expanded || count <= DEVICE_CARD.maxAccountIcons) return { visible: count, more: false }
  return { visible: DEVICE_CARD.maxAccountIcons - 1, more: true }
}

export type DeviceLoginHealth = `ok` | `needs_relogin` | `signed_out` | `unknown`

export interface DeviceListLogin {
  agent: string
  profileId: string
  /** `loginLabel` — the email, else the plan, else the fallback. */
  label: string
  health: DeviceLoginHealth
  /** EXP-1560: a fresh usage window at ≥ 100%. */
  limitHit?: boolean
  /** EXP-1600: when the profile's long-lived claude login ends (ISO); absent
   *  = it holds none. */
  tokenExpiresAt?: string | null
}

export interface DeviceListInput {
  online: boolean
  /** The caller's own machine (not a teammate's shared server). */
  mine: boolean
  /** A headless CLI server — the only kind that acts on an update request
   * (a desktop app updates itself). */
  server: boolean
  registered: boolean
  /** The client's own version compare said the device is behind `latest`. */
  outdated: boolean
  latest: string | null
  updateQueued: boolean
  /** The machine sent its accounts at least once (`agentAccounts` present). */
  accountsReported: boolean
  /** Every login PROFILE it reported; an agent with none adds nothing. */
  logins: readonly DeviceListLogin[]
  /** EXP-1600: the clock the renewal reminder reads (ISO). */
  now: string
}

export type DeviceAttentionItem =
  | {
      kind: `login`
      agent: string
      profileId: string
      text: string
      action: `sign_in` | `keep_signed_in` | null
      /** EXP-1560: the preview's status line (`copy.status`). */
      status: `needs_relogin` | `signed_out` | `limit` | `renew` | `switch`
    }
  | { kind: `update`; text: string; action: `update` | null }
  | { kind: `update_queued`; text: string; action: null }

export interface DeviceListRow {
  group: `online` | `offline`
  noAccount: boolean
  attention: DeviceAttentionItem[]
}

export function deviceListRow(input: DeviceListInput): DeviceListRow {
  const attention: DeviceAttentionItem[] = []
  const canSignIn = input.mine && input.online
  for (const [health, template] of [
    [`needs_relogin`, DEVICES_LIST_COPY.needsRelogin],
    [`signed_out`, DEVICES_LIST_COPY.signedOut],
  ] as const) {
    for (const login of input.logins) {
      if (login.health !== health) continue
      attention.push({
        kind: `login`,
        agent: login.agent,
        profileId: login.profileId,
        text: template.replace(`{label}`, login.label),
        action: canSignIn ? `sign_in` : null,
        status: health,
      })
    }
  }
  const dead = (login: DeviceListLogin) =>
    login.health === `needs_relogin` || login.health === `signed_out`
  for (const login of input.logins) {
    if (!login.limitHit || dead(login)) continue
    attention.push({
      kind: `login`,
      agent: login.agent,
      profileId: login.profileId,
      text: DEVICES_LIST_COPY.limitReached.replace(`{label}`, login.label),
      action: null,
      status: `limit`,
    })
  }
  // EXP-1600: the yearly renewal reminder, then the switch prompt for a
  // claude login made before Keep signed in became the default.
  if (input.mine) {
    const quiet = (login: DeviceListLogin) =>
      login.agent !== `claude` || dead(login) || login.limitHit === true
    for (const [status, template] of [
      [`renew`, DEVICES_LIST_COPY.renewSoon],
      [`switch`, DEVICES_LIST_COPY.switchPrompt],
    ] as const) {
      for (const login of input.logins) {
        if (quiet(login)) continue
        const due =
          status === `renew`
            ? Boolean(login.tokenExpiresAt) && keepSignedInDue(login.tokenExpiresAt!, input.now)
            : !login.tokenExpiresAt
        if (!due) continue
        attention.push({
          kind: `login`,
          agent: login.agent,
          profileId: login.profileId,
          text: template.replace(`{label}`, login.label),
          action: canSignIn ? `keep_signed_in` : null,
          status,
        })
      }
    }
  }
  if (input.mine) {
    if (input.updateQueued) {
      attention.push({ kind: `update_queued`, text: DEVICES_LIST_COPY.updateQueued, action: null })
    } else if (input.outdated && input.latest) {
      attention.push({
        kind: `update`,
        text: DEVICES_LIST_COPY.updateAvailable.replace(`{version}`, input.latest),
        action: input.server && input.online && input.registered ? `update` : null,
      })
    }
  }
  return {
    group: input.online ? `online` : `offline`,
    noAccount: input.accountsReported && input.logins.length === 0,
    attention,
  }
}
