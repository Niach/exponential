import { AgentMark, DeviceAttention, Pill, getDeviceIcon, type DeviceAttentionLine } from "@exp/ui"
import { DEVICES_LIST_COPY } from "@exp/domain-contract/devices-list"

import type { StyleguideEntry } from "./types.ts"

// A-16: THE Devices page row and its attention card (`devices-list.json`).
// The REAL `@exp/ui` `DeviceAttention` beside a row drawn from the same
// pieces the page uses (Pill chips with the agent's brand mark).

const LaptopIcon = getDeviceIcon({ kind: `desktop` })
const noop = () => {}

const ITEMS: DeviceAttentionLine[] = [
  {
    key: `relogin`,
    text: DEVICES_LIST_COPY.needsRelogin.replace(`{label}`, `danny@yourev.at`),
    action: { label: DEVICES_LIST_COPY.signIn, onSelect: noop },
  },
  {
    key: `update`,
    text: DEVICES_LIST_COPY.updateAvailable.replace(`{version}`, `0.14.79`),
    action: { label: DEVICES_LIST_COPY.update, onSelect: noop },
  },
]

const LOGINS = [`dennis@yourev.at`, `danny@yourev.at`]

export const entry: StyleguideEntry = {
  id: `device-attention`,
  section: `special`,
  owner: `A-16`,
  title: `Device row + attention`,
  blurb: `A-16 (devices-list.json): one line per device, nothing folds — glyph, name + version + status, its logins as chips (brand mark, email · plan; the account picker's limits preview on hover, the account menu on click or tap), then ONE warning-tone attention icon when something needs a fix, then the gear. The icon opens a small card on hover (pointer) or tap (touch): one line per item, its text and at most one action pill — dead logins first, then signed-out ones, then the update. An agent CLI that was never signed in is not an item. A device with no login says "No account"; offline rows sink under an "Offline" label, dimmed.`,
  status: {
    web: { state: `ok`, symbol: `DeviceAttention`, file: `packages/ui/src/device-attention.tsx` },
    desktop: { state: `ok`, symbol: `device_row`, file: `apps/desktop/crates/ui/src/machines.rs` },
    ios: { state: `ok`, symbol: `DeviceAttentionMenu`, file: `apps/ios/Exponential/UI/Session/DeviceLogins.swift` },
    android: { state: `ok`, symbol: `DeviceAttentionButton`, file: `apps/android/app/src/main/java/com/exponential/app/ui/session/AgentsScreen.kt` },
  },
  island: () => (
    <div className="flex min-h-56 w-full max-w-[44rem] items-start gap-3 rounded-md p-3">
      <LaptopIcon className="mt-0.5 size-4 shrink-0 text-foreground/70" />
      <div className="w-40 min-w-0 shrink-0">
        <div className="flex items-baseline gap-1.5">
          <span className="text-sm font-medium">macbook</span>
          <span className="text-[10px] text-amber-500">v0.14.78</span>
        </div>
        <div className="text-xs text-muted-foreground">Online</div>
      </div>
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
        {LOGINS.map((email) => (
          <Pill key={email} leading={<AgentMark agent="claude" className="size-3.5" />}>
            {email}
            <span className="text-muted-foreground/60"> · max</span>
          </Pill>
        ))}
      </div>
      <DeviceAttention items={ITEMS} label={DEVICES_LIST_COPY.attention} defaultOpen />
    </div>
  ),
}
