import type { ReactNode } from "react"
import { Button, StatusGlyph, conceptIcon } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1255 (EXP-1259 D3): the work tab strip (web md+, desktop). The strip
// itself lives in apps/web (`components/team/work-tabs-strip.tsx`) over the
// pure `lib/work-tabs.ts`, so the specimen draws its chips from the same
// `@exp/ui` Button + concept glyphs at rest: the × at rest is hidden on every
// chip and shows on the HOVERED one (here the second, the active one keeps
// no exception), the "+N" overflow, then the ghost "+" that opens the New
// run tab, drawn last as an open New run chip.

const UiCloseIcon = conceptIcon(`ui-close`)
const UiAddIcon = conceptIcon(`ui-add`)
const NewRunIcon = conceptIcon(`action-chat`)
const noop = (): void => {}

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
    <div
      className={`flex h-8 max-w-[15rem] shrink-0 items-center rounded-md border ${
        active ? `border-border bg-muted/50` : `border-transparent`
      } ${hovered ? `bg-muted/30` : ``}`}
    >
      <Button variant="ghost" size="sm" className="h-8 min-w-0 flex-1 justify-start gap-1.5 bg-transparent! pr-1 pl-2 font-normal">
        {lead}
        {identifier && <span className="font-mono text-xs text-muted-foreground">{identifier}</span>}
        <span className="truncate">{title}</span>
      </Button>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label="Close tab"
        className={`mr-1 shrink-0 ${hovered ? `` : `opacity-0`}`}
        onClick={noop}
      >
        <UiCloseIcon />
      </Button>
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `work-tabs-strip`,
  section: `special`,
  owner: `EXP-1255`,
  title: `Work tab strip`,
  blurb: `The tabs over the content card (web md+ and the IDE; phones have none). An issue and its run are ONE tab. The × shows only while a tab is hovered, every tab alike including the active one (keyboard focus shows it too; the close chord is unchanged). Chips that do not fit fold into "+N". A ghost "+" after the last tab opens ONE New run tab hosting the home composer (IDE Cmd/Ctrl+T; web Alt/Option+T, since browsers keep Cmd/Ctrl+T); once its run starts the tab turns into that run's tab. Runs started anywhere else only show in Running. Switching tabs never animates.`,
  status: {
    web: {
      state: `ok`,
      symbol: `WorkTabsStrip`,
      file: `apps/web/src/components/team/work-tabs-strip.tsx`,
      note: `rules: lib/work-tabs.ts (openNewRunTab, landNewRun); the chord: work-tabs-sync.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `render_tab_strip / open_new_run_tab`,
      file: `apps/desktop/crates/ui/src/screens.rs`,
    },
    ios: { state: `n/a`, note: `phones have no tabs: one Work screen` },
    android: { state: `n/a`, note: `phones have no tabs: one Work screen` },
  },
  island: () => (
    <div className="flex items-center gap-1">
      <Chip lead={<StatusGlyph icon="progress-2-4" colorClass="text-amber-500 size-3.5" />} identifier="EXP-1223" title="ExponentialOS" active />
      <Chip lead={<StatusGlyph icon="circle-dashed" colorClass="text-muted-foreground size-3.5" />} identifier="EXP-563" title="google keywords" hovered />
      <Chip lead={<NewRunIcon className="size-3.5 text-muted-foreground" />} title="New run" />
      <Button variant="ghost" size="sm" className="h-8 px-2 text-muted-foreground">
        +2
      </Button>
      <Button variant="ghost" size="icon-xs" aria-label="New run" className="text-muted-foreground">
        <UiAddIcon />
      </Button>
    </div>
  ),
}
