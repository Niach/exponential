import {
  AgentRunMark,
  Button,
  DeviceAccountPanel,
  GlassCard,
  conceptIcon,
} from "@exp/ui"
import { DEVICE_CARD, DEVICES_LIST_COPY } from "@exp/domain-contract/devices-list"

import type { StyleguideEntry } from "./types.ts"

// EXP-1560 (was A-16): THE Devices page card (`devices-list.json`), drawn
// from the pieces the page uses: GlassCard, the device icon, the ⋯ ghost button,
// the logins as AgentRunMark icons (the amber `needs_input` badge ON the login
// that needs attention) and #EXP-1643 THE account panel, open on the badged icon.

const DeviceIcon = conceptIcon(`ui-device`)
const MoreIcon = conceptIcon(`ui-more`)
const AddIcon = conceptIcon(`ui-add`)

const LOGINS: { agent: string; attention: boolean }[] = [
  { agent: `claude`, attention: false },
  { agent: `claude`, attention: true },
  { agent: `codex`, attention: false },
]

const WINDOWS = [
  { key: `session`, label: `Current session`, percent: 42, caption: `resets in 2h 10m` },
  { key: `weekly`, label: `All models`, percent: 81, caption: `resets in 3d 14h` },
  { key: `model:fable`, label: `Fable only`, percent: 64, caption: `resets in 3d 14h` },
]

const noop = () => {}

function Icon({ agent, attention, open = false }: { agent: string; attention: boolean; open?: boolean }) {
  return (
    <Button variant="ghost" size="icon-sm" className={open ? `size-7 bg-glass-active` : `size-7`} aria-label={agent}>
      <AgentRunMark
        agent={agent}
        state={attention ? `needs_input` : undefined}
        ringClassName="ring-card-opaque"
        className="size-4 [&>svg]:size-4"
      />
    </Button>
  )
}

function Card({ name, version, outdated, status, preview }: {
  name: string
  version: string
  outdated?: boolean
  status: string
  preview?: boolean
}) {
  return (
    <GlassCard className="flex min-w-0 flex-col gap-3" style={{ padding: DEVICE_CARD.padding }}>
      <div className="flex min-w-0 items-start gap-2">
        <DeviceIcon
          aria-hidden
          className="mt-px shrink-0 text-muted-foreground"
          style={{ width: DEVICE_CARD.deviceIconSize, height: DEVICE_CARD.deviceIconSize }}
        />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-baseline gap-1.5">
            <span className="min-w-0 truncate text-sm font-medium">{name}</span>
            <span className={`shrink-0 text-[10px] ${outdated ? `text-yellow-400` : `text-muted-foreground/60`}`}>
              v{version}
            </span>
          </div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span className="size-1.5 rounded-full bg-emerald-500" />
            {status}
          </div>
        </div>
        <Button variant="ghost" size="icon-sm" className="-mt-1 -mr-1 size-7 shrink-0 text-muted-foreground" aria-label={DEVICES_LIST_COPY.deviceActions}>
          <MoreIcon className="size-4" />
        </Button>
      </div>
      <div className="flex flex-wrap items-center gap-1">
        {LOGINS.map((login, index) => (
          <Icon key={index} {...login} open={preview && login.attention} />
        ))}
        <Button variant="ghost" size="icon-sm" className="size-7 text-muted-foreground" aria-label={DEVICES_LIST_COPY.addAccount}>
          <AddIcon className="size-4" />
        </Button>
      </div>
      {preview && (
        <DeviceAccountPanel
          name="alex@acme.example"
          plan="Max"
          status={DEVICES_LIST_COPY.status.needs_relogin}
          windows={WINDOWS}
          signIn={{ label: DEVICES_LIST_COPY.signIn, onSelect: noop }}
          signOut={{ label: `Sign out`, onSelect: noop }}
          remove={{ label: `Remove account`, onSelect: noop }}
        />
      )}
    </GlassCard>
  )
}

export const entry: StyleguideEntry = {
  id: `device-attention`,
  section: `special`,
  owner: `EXP-1560`,
  title: `Device card`,
  blurb: `#EXP-1643 (devices-list.json): the Devices page is a grid of cards (min 256, gap 12; 1 column on a portrait phone), aligned to the top. A card: the device's own icon, name + version (amber when outdated) over the status, a ⋯ overflow (phones: the bottom sheet), then the logins as icons with THE run-mark amber badge on one that needs attention. Pressing an icon expands the card with THE account panel: email + icon-only Sign out and Remove, plan + the one action pill (Sign in or Persist login), the status line, the usage windows. The open login is remembered per device on this client; a device with one login opens it by default. No hover card, no account menu.`,
  status: {
    web: { state: `ok`, symbol: `DeviceCard`, file: `apps/web/src/components/my-machines.tsx` },
    desktop: { state: `ok`, symbol: `device_card`, file: `apps/desktop/crates/ui/src/machines.rs` },
    ios: { state: `ok`, symbol: `DeviceCardMenu`, file: `apps/ios/Exponential/UI/Session/DeviceLogins.swift` },
    android: { state: `ok`, symbol: `DeviceCard`, file: `apps/android/app/src/main/java/com/exponential/app/ui/session/AgentsScreen.kt` },
  },
  island: () => (
    <div
      className="grid min-h-72 w-full max-w-[44rem] items-start p-3"
      style={{ gridTemplateColumns: `repeat(auto-fill, minmax(${DEVICE_CARD.minWidth}px, 1fr))`, gap: DEVICE_CARD.gap }}
    >
      <Card name="macbook" version="0.14.78" outdated status="Online" preview />
      <Card name="mint" version="0.14.79" status="Online" />
    </div>
  ),
}
