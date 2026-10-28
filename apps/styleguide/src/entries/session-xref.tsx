import type { SessionXref as SessionXrefModel } from "@exp/domain-contract/session-tree"
import { SessionXref } from "@exp/ui"

import type { ReactNode } from "react"

import type { StyleguideEntry } from "./types.ts"

// S-3 inter-session links: THE link line (`packages/ui/src/session-xref.tsx`),
// case for case with the desktop twin (`styleguide/entries/session_xref.rs`):
// the four kinds, two through an agent's avatar, two through the session
// glyph, then one whose long name truncates.

const BIG_BROTHER = { icon: `bot`, color: `#8b5cf6` }

const xref = (kind: SessionXrefModel[`kind`], name: string, agentId: string | null = null): SessionXrefModel => ({
  kind,
  sessionId: `sg-xref-${kind}`,
  name,
  agentId,
})

function Captioned({ caption, children }: { caption: string; children: ReactNode }) {
  return (
    <div className="flex flex-col items-start gap-1">
      {children}
      <div className="text-xs text-muted-foreground">{caption}</div>
    </div>
  )
}

const noop = (): void => {}

export const entry: StyleguideEntry = {
  id: `session-xref`,
  section: `general`,
  owner: `S-3`,
  title: `Session link`,
  blurb: `A row that came from ANOTHER session says so in one compact line, identical on all four clients: the muted lead (\`Message from\` · \`Report from\` · \`Started by\` · \`Reported to\`, \`session-thread.json\` \`copy.xref\`), the session's mark (the agent's avatar at 16 px when its tree has an agent, else the session glyph \`coding-assistant\`), the session's name (foreground, medium, truncating) and a muted chevron, 4 px apart, text-xs. The whole line is a button (hover = foreground, test id \`thread-xref\`): a tap opens that session and pushes the one on show on the back stack (web: the route's \`back\`, the header's ‹ pops it; phones and the IDE: their navigation push). A message from another session carries it above its bubble; a child's report IS the line; the session view and the Conversation open with the viewed session's origin (\`Started by\`, \`Reported to\` once it ended). Model = \`buildSessionThread\` (\`xref\`, \`origin\`, fixture \`session-thread.json\` ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `SessionXref`,
      file: `packages/ui/src/session-xref.tsx`,
      note: `rows: session-thread-view.tsx; back stack: apps/web/src/lib/session-back.ts`,
    },
    desktop: {
      state: `ok`,
      symbol: `session_xref`,
      file: `apps/desktop/crates/ui/src/session_xref.rs`,
      note: `styleguide/entries/session_xref.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `SessionXrefHeader`,
      file: `apps/ios/ExpUI/Sources/SessionXrefHeader.swift`,
    },
    android: {
      state: `ok`,
      symbol: `SessionXref`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/SessionXref.kt`,
    },
  },
  island: () => (
    <div className="flex max-w-[420px] flex-col gap-5">
      <Captioned caption="message: another session's line, above its bubble">
        <SessionXref xref={xref(`message`, `Big Brother`, `a1`)} agent={BIG_BROTHER} onOpen={noop} />
      </Captioned>
      <Captioned caption="report: a child's end report, the whole row">
        <SessionXref xref={xref(`report`, `Batch 2`)} onOpen={noop} />
      </Captioned>
      <Captioned caption="started: the agent conversation that started this session">
        <SessionXref xref={xref(`started`, `Big Brother`, `a1`)} agent={BIG_BROTHER} onOpen={noop} />
      </Captioned>
      <Captioned caption="reported: an ended session's parent">
        <SessionXref xref={xref(`reported`, `Fix the checkout`)} onOpen={noop} />
      </Captioned>
      <Captioned caption="a long name truncates on one line">
        <SessionXref
          xref={xref(`message`, `Migrate every board's shape proxy to the new columns allowlist`)}
          onOpen={noop}
        />
      </Captioned>
    </div>
  ),
}
