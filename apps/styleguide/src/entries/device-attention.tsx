import {
  AgentRunMark,
  Button,
  GlassCard,
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
  Pill,
  UsageWindows,
  conceptIcon,
} from "@exp/ui"
import { DEVICE_CARD, DEVICES_LIST_COPY } from "@exp/domain-contract/devices-list"

import type { StyleguideEntry } from "./types.ts"

// EXP-1560 (was A-16): THE Devices page card (`devices-list.json`), drawn
// from the pieces the page uses: GlassCard, the ⋯ ghost button, the logins as
// AgentRunMark icons (the amber `needs_input` badge ON the login that needs
// attention) and its preview — email, plan, status, UsageWindows `hover`,
// Sign in — open under the badged icon.

const MoreIcon = conceptIcon(`ui-more`)
const AddIcon = conceptIcon(`ui-add`)

const LOGINS: { agent: string; attention: boolean }[] = [
  { agent: `claude`, attention: false },
  { agent: `claude`, attention: true },
  { agent: `codex`, attention: false },
]

const WINDOWS = [
  { key: `session`, label: `5h`, percent: 42, caption: `resets in 2h 10m` },
  { key: `weekly`, label: `Week`, percent: 81, caption: `resets in 3d 14h` },
  { key: `model:fable`, label: `Fable`, percent: 64, caption: `resets in 3d 14h` },
]

function Icon({ agent, attention }: { agent: string; attention: boolean }) {
  return (
    <Button variant="ghost" size="icon-sm" className="size-7" aria-label={agent}>
      <AgentRunMark
        agent={agent}
        state={attention ? `needs_input` : undefined}
        ringClassName="ring-glass-card-opaque"
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
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-baseline gap-1.5">
            <span className="min-w-0 truncate text-sm font-medium">{name}</span>
            <span className={`shrink-0 text-[10px] ${outdated ? `text-amber-500` : `text-muted-foreground/60`}`}>
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
        {LOGINS.map((login, index) =>
          preview && login.attention ? (
            <HoverCard key={index} open>
              <HoverCardTrigger asChild>
                <span className="inline-flex">
                  <Icon {...login} />
                </span>
              </HoverCardTrigger>
              <HoverCardContent side="bottom" align="start" sideOffset={6} className="w-auto p-3">
                <div className="flex w-48 flex-col gap-2">
                  <div className="flex min-w-0 flex-col">
                    <span className="truncate text-sm font-medium">danny@yourev.at</span>
                    <span className="truncate text-xs text-muted-foreground">max</span>
                    <span className="text-xs text-amber-500">{DEVICES_LIST_COPY.status.needs_relogin}</span>
                  </div>
                  <UsageWindows windows={WINDOWS} density="hover" />
                  <Pill mode="action" className="self-start">
                    {DEVICES_LIST_COPY.signIn}
                  </Pill>
                </div>
              </HoverCardContent>
            </HoverCard>
          ) : (
            <Icon key={index} {...login} />
          )
        )}
        <Button variant="ghost" size="icon-sm" className="size-7 text-muted-foreground" aria-label={DEVICES_LIST_COPY.addAccount}>
          <AddIcon className="size-4" />
        </Button>
      </div>
    </GlassCard>
  )
}

export const entry: StyleguideEntry = {
  id: `device-attention`,
  section: `special`,
  owner: `EXP-1560`,
  title: `Device card`,
  blurb: `EXP-1560 (devices-list.json): the Devices page is a grid of cards (min 256, gap 12; 1 column on a portrait phone). A card: name + version (amber when outdated) over the status, a ⋯ overflow top-right (the update line, Update, Device settings), and the logins along the bottom as brand-mark icons only — the amber run-mark badge ON a login that is dead, signed out or at its limit, a ⋯ past five, then +. Hovering (pointer) or tapping an icon previews the login: email, plan, its status, the usage windows, Sign in.`,
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
