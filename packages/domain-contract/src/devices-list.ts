// A-16: the Devices page row — which group a device sits in, whether it says
// "No account", and what its attention card lists. Pure and byte-locked by
// `fixtures/devices-list.json` (`cases`), which every mirror replays: desktop
// `ui::machines` (`device_attention`), iOS `ExpCore/Sources/Domain/DevicesList.swift`,
// Android `domain/DevicesList.kt`. The fixture's `$comment` holds the rules;
// this is the reference.

import fixture from "../fixtures/devices-list.json" with { type: "json" }

export const DEVICES_LIST_COPY = fixture.copy

export type DeviceLoginHealth = `ok` | `needs_relogin` | `signed_out` | `unknown`

export interface DeviceListLogin {
  agent: string
  profileId: string
  /** `loginLabel` — the email, else the plan, else the fallback. */
  label: string
  health: DeviceLoginHealth
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
}

export type DeviceAttentionItem =
  | {
      kind: `login`
      agent: string
      profileId: string
      text: string
      action: `sign_in` | null
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
      })
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
