import type { ReactNode } from "react"
import { Button, StatusGlyph, StripChip, StripOverflowButton, conceptIcon } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1255 (EXP-1259 D3): the work tab strip (web md+, desktop). The strip
// itself lives in apps/web (`components/team/work-tabs-strip.tsx`) over the
// pure `lib/work-tabs.ts`, so the specimen draws its chips from the same
// `@exp/ui` strip chip (`StripChip`, EXP-1334: ONE tab component with the
// thread header's strip) + concept glyphs at rest: the × at rest is hidden on every
// chip and shows on the HOVERED one (here the second, the active one keeps
// no exception), the "+N" overflow, then the ghost "+" that opens the New
// session tab, drawn last as an open New session chip.

const UiCloseIcon = conceptIcon(`ui-close`)
const UiAddIcon = conceptIcon(`ui-add`)
const NewRunIcon = conceptIcon(`action-chat`)
const noop = (): void => {}

/** THE strip chip (EXP-1334: the thread header's chip box too), the ×
 *  as its trailing part, hidden unless the tab is hovered. */
function Chip({
  lead,
  identifier,
  title,
  active,
  hovered,
}: {
  lead: ReactNode
  identifier?: string
  title: string
  active?: boolean
  hovered?: boolean
}) {
  return (
    <StripChip
      active={active}
      lead={lead}
      identifier={identifier}
      label={title}
      boxClassName={hovered ? `bg-glass-active text-foreground` : undefined}
      trailing={
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label="Close tab"
          className={`mr-1 shrink-0 ${hovered ? `` : `opacity-0`}`}
          onClick={noop}
        >
          <UiCloseIcon />
        </Button>
      }
    />
  )
}

export const entry: StyleguideEntry = {
  id: `work-tabs-strip`,
  section: `special`,
  owner: `EXP-1255`,
  title: `Work tab strip`,
  blurb: `The tabs over the content card (web md+ and the IDE; phones have none). Route-driven (EXP-1293): an issue, a thread, an agent's channel or a raw session the URL opened is a tab; a thread chip leads with its agent mark. The × shows only while a tab is hovered, every tab alike including the active one (keyboard focus shows it too; the close chord is unchanged). Every chip is THE strip chip (EXP-1334: the same box as the thread header's chips, \`StripChip\`); chips that do not fit fold into "+N" (the contract's \`partitionTabs\`). A ghost "+" after the last tab opens ONE New session tab hosting the home composer (IDE Cmd/Ctrl+T; web Alt/Option+T, since browsers keep Cmd/Ctrl+T); the navigation that leaves it (its start opening the thread) lands in that tab. Threads opened from the sidebar's Running stay tabless. Switching tabs never animates.`,
  status: {
    web: {
      state: `ok`,
      symbol: `WorkTabsStrip`,
      file: `apps/web/src/components/team/work-tabs-strip.tsx`,
      note: `rules: lib/work-tabs.ts (upsertFromRoute + landing, openNewRunTab); the chord: work-tabs-sync.tsx`,
    },
    desktop: {
      state: `leftover`,
      symbol: `render_tab_strip / home_button`,
      file: `apps/desktop/crates/ui/src/screens.rs`,
      note: `EXP-1294: the "+" opens Home (open_home), no New session tab`,
    },
    ios: { state: `n/a`, note: `phones have no tabs: one Work screen` },
    android: { state: `n/a`, note: `phones have no tabs: one Work screen` },
  },
  island: () => (
    <div className="flex items-center gap-1">
      <Chip lead={<StatusGlyph icon="progress-2-4" colorClass="text-amber-500 size-3.5" />} identifier="EXP-1223" title="ExponentialOS" active />
      <Chip lead={<StatusGlyph icon="circle-dashed" colorClass="text-muted-foreground size-3.5" />} identifier="EXP-563" title="google keywords" hovered />
      <Chip lead={<NewRunIcon className="size-3.5 text-muted-foreground" />} title="New session" />
      <StripOverflowButton count={2} aria-label="2 more tabs" />
      <Button variant="ghost" size="icon-xs" aria-label="New session" className="text-muted-foreground">
        <UiAddIcon />
      </Button>
    </div>
  ),
}
