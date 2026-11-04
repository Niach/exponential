import type { ReactNode } from "react"
import { EntityRefChipView, StatusGlyph } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1674: THE entity chip (EXP-920), the issue chip's box for everything an
// answer or a link can name: the REAL `EntityRefChipView` per kind, an issue
// with its status glyph, and the muted row a viewer has not synced. Agent prose
// paints a bare in-app URL as this chip (prose-links.json).

const noop = (): void => {}

function Captioned({ caption, children }: { caption: string; children: ReactNode }) {
  return (
    <div className="grid content-start gap-2">
      <div className="flex items-center">{children}</div>
      <p className="text-xs text-muted-foreground">{caption}</p>
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `entity-chip`,
  section: `general`,
  owner: `EXP-1674`,
  title: `Entity chip`,
  blurb: `The issue chip's box, opened to every kind Exponential can name: a settled tool row draws one chip per thing its answer touched, and agent prose paints a bare in-app URL as this chip with the row's title. The glyph is the kind's concept (an issue: its status), the label the contract's. A row this viewer has not synced stays greyed with no target; in prose it stays a plain link. A link the agent labelled keeps its text and the link style. Exponential-specific, so it is not in Exponential UI.`,
  status: {
    web: {
      state: `ok`,
      symbol: `EntityChip / EntityRefChipView`,
      file: `packages/ui/src/entity-chip.tsx`,
      note: `the app binds rows and targets in components/entity-preview/; agent prose: the decoration in apps/web/src/lib/issue-ref-extension.ts`,
    },
    desktop: {
      state: `ok`,
      symbol: `entity_chip`,
      file: `apps/desktop/crates/ui/src/entity_chip.rs`,
      note: `agent prose: DecorationStyle::EntityPill in markdown/editor.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `EntityChip`,
      file: `apps/ios/ExpUI/Sources/EntityChip.swift`,
      note: `agent prose: IssueRefs.decorateProseChips`,
    },
    android: {
      state: `ok`,
      symbol: `EntityChip`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/EntityChip.kt`,
      note: `agent prose: the chip marks of annotateLine in ui/markdown/MarkdownView.kt`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-6">
      <Captioned caption="an issue: its status glyph, identifier and title">
        <EntityRefChipView
          entityRef={{ kind: `issue`, id: `i-1`, identifier: `EXP-42`, title: `Fix the flicker` }}
          icon={<StatusGlyph icon="progress-2-4" colorClass="text-amber-500 !h-[0.875em] !w-[0.875em]" />}
          onClick={noop}
        />
      </Captioned>
      <Captioned caption="a board">
        <EntityRefChipView entityRef={{ kind: `board`, id: `b-1`, title: `Web` }} onClick={noop} />
      </Captioned>
      <Captioned caption="a session">
        <EntityRefChipView entityRef={{ kind: `session`, id: `s-1`, title: `Fix the flicker` }} onClick={noop} />
      </Captioned>
      <Captioned caption="an agent">
        <EntityRefChipView entityRef={{ kind: `agent`, id: `a-1`, title: `Triage` }} onClick={noop} />
      </Captioned>
      <Captioned caption="not synced: greyed, no target">
        <EntityRefChipView entityRef={{ kind: `board`, id: `b-2`, title: `Mobile` }} muted />
      </Captioned>
    </div>
  ),
}
