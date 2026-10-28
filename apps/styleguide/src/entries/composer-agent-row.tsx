import { useState } from "react"
import home from "@exp/domain-contract/fixtures/composer-home.json"
import { composerAgentRow } from "@exp/domain-contract/composer-agent-row"
import { ComposerAgentRow } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// S-3 (composer-home.json `agentRow`): THE agent row over the New-chat
// composer card, the real `@exp/ui` `ComposerAgentRow` fed the fixture's
// first case (the viewer's Assistant first and preselected, another
// member's Assistant out, the dotted Temporary session circle last).

const row = home.agentRow
const sample = row.cases[0]!

function Specimen() {
  const [picked, setPicked] = useState<string | null>(null)
  const state = composerAgentRow({
    agents: sample.agents.map((agent) => ({ ...agent, icon: null, color: null })),
    me: sample.me,
    picked,
  })
  return (
    <div className="flex max-w-[40rem] flex-col gap-3">
      <ComposerAgentRow row={state} onPick={setPicked} />
      <div className="h-24 rounded-2xl border border-glass-stroke-card bg-glass-card" aria-hidden />
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `composer-agent-row`,
  section: `special`,
  owner: `S-3`,
  title: `Composer agent row`,
  blurb: `THE agent row over the New-chat composer card ×4 (Exponential-specific, not an Exponential UI primitive): the muted label line ("${row.copy.to.replace(`{name}`, `Assistant`)}" / "${row.copy.temporary}") ${row.labelToRow}px over single-select circles, ${row.rowToCard}px over the card. Each circle = the Chats agent bubble at ${row.size}px (the disc on bg muted inside a ${row.ringGap}px background gap; no name under it): the viewer's Assistant first (THE brand mark) and preselected, then the team agents (icon in its colour), then ONE dotted circle = Temporary session (a normal launch). The selected circle wears the ${row.ringWidth}px foreground ring, the others the glass hairline. An agent picked hides the + menu's runner and work rows and the send messages that agent (sessions.send({agentId})), then opens its chat.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ComposerAgentRow`,
      file: `packages/ui/src/composer-agent-row.tsx`,
      note: `mounted by apps/web/src/components/launch-composer.tsx on the agent home (model: hooks/use-launch-composer.ts agentRow / pickRecipient)`,
    },
    desktop: {
      state: `ok`,
      symbol: `composer_agent_row`,
      file: `apps/desktop/crates/ui/src/chat_list.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `ComposerAgentRowView`,
      file: `apps/ios/ExpUI/Sources/ComposerAgentRowView.swift`,
    },
    android: {
      state: `ok`,
      symbol: `ComposerAgentRowView`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/agent/ComposerAgentRowView.kt`,
    },
  },
  island: () => <Specimen />,
}
