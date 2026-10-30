import type { ReactNode } from "react"
import { AgentRunMark, Button, StatusGlyph, StripChip, StripOverflowButton, conceptIcon } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1334: THE strip chip, the one tab box of every strip of chips (the
// thread header's issue + session chips, the work tab strip's tabs) and the
// "+N" of a folded zone, drawn bare: the REAL `StripChip` /
// `StripOverflowButton` in every state the two strips put them in. The
// composed strips are the "Work tab strip" and "Thread" islands.

const UiCloseIcon = conceptIcon(`ui-close`)
const noop = (): void => {}

const inProgress = <StatusGlyph icon="progress-2-4" colorClass="text-amber-500 size-3.5" />
const backlog = <StatusGlyph icon="circle-dashed" colorClass="text-muted-foreground size-3.5" />
const done = <StatusGlyph icon="circle-check" colorClass="text-emerald-500 size-3.5" />

function Captioned({ caption, children }: { caption: string; children: ReactNode }) {
  return (
    <div className="grid content-start gap-2">
      <div className="flex items-center">{children}</div>
      <p className="text-xs text-muted-foreground">{caption}</p>
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `strip-chip`,
  section: `general`,
  owner: `EXP-1334`,
  title: `Strip chip`,
  blurb: `The one chip of every strip of chips, identical in the thread header (issue and session chips) and the work tab strip (web md+, the IDE): the strip tab box (32 px, text-sm, rounded-md, a hairline at rest, the glass wash while active), inside it the lead glyph (an issue's status glyph, a session's agent mark), the mono identifier, the label truncating at 15rem, then the open nested item as "chip › open one" and a chevron-down on a chip that opens its subtree on a click instead of itself. A trailing control (the work tab's x) sits inside the box after the open button. "short" is the identifier alone, the tier a strip falls to before it folds; what does not fit folds into the "+N" overflow button, the same box at rest holding the count, the trigger of THE Menu listing the hidden chips.`,
  status: {
    web: {
      state: `ok`,
      symbol: `StripChip / StripOverflowButton`,
      file: `packages/ui/src/strip-chip.tsx`,
      note: `the thread header binds it as ThreadIssueTab / ThreadStripGroup (session-thread-view.tsx); the work tab strip as its tabs`,
    },
    desktop: {
      state: `ok`,
      symbol: `thread_screen::strip_chip`,
      file: `apps/desktop/crates/ui/src/thread_screen.rs`,
      note: `the thread header's chip frame and its "+N"; the work tab strip's tabs are surface::rich_tab, not yet this one frame`,
    },
    ios: {
      state: `ok`,
      symbol: `ThreadStripTabs.issueChip / sessionChip / overflowChip`,
      file: `apps/ios/Exponential/UI/Thread/ThreadStripTabs.swift`,
      note: `one painter per chip kind, no shared primitive; phones have no work tab strip`,
    },
    android: {
      state: `ok`,
      symbol: `IssueStripChip / SessionStripChip / MoreChip`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/thread/ThreadParts.kt`,
      note: `one composable per chip kind, no shared primitive; phones have no work tab strip`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-6">
      <Captioned caption="at rest">
        <StripChip lead={backlog} identifier="EXP-563" label="google keywords" onClick={noop} />
      </Captioned>
      <Captioned caption="active">
        <StripChip lead={inProgress} identifier="EXP-1223" label="ExponentialOS" active onClick={noop} />
      </Captioned>
      <Captioned caption="short (the identifier alone)">
        <StripChip lead={done} identifier="EXP-1334" onClick={noop} />
      </Captioned>
      <Captioned caption="folded: path and chevron (a click opens its tree)">
        <StripChip
          lead={inProgress}
          identifier="EXP-1334"
          label="Thread header"
          path="EXP-1340"
          chevron
          active
          onClick={noop}
        />
      </Captioned>
      <Captioned caption="a session chip: the agent mark, the open nested session">
        <StripChip
          lead={<AgentRunMark agent="claude" state="working" ringClassName="ring-background" className="size-3.5" />}
          label="Threads impl"
          path="Android lane"
          chevron
          onClick={noop}
        />
      </Captioned>
      <Captioned caption="with a trailing x (the work tab)">
        <StripChip
          lead={backlog}
          identifier="EXP-1255"
          label="Work tab strip"
          onClick={noop}
          trailing={
            <Button variant="ghost" size="icon-xs" aria-label="Close tab" className="mr-1 shrink-0" onClick={noop}>
              <UiCloseIcon />
            </Button>
          }
        />
      </Captioned>
      <Captioned caption="the +N overflow">
        <StripOverflowButton count={3} aria-label="3 more issues" onClick={noop} />
      </Captioned>
    </div>
  ),
}
