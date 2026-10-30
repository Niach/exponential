import fixture from "@exp/domain-contract/fixtures/session-thread.json"
import { buildSessionThread, type ComposerReplies as ComposerRepliesModel } from "@exp/domain-contract/session-tree"
import { ThreadConversation, TooltipProvider, userMessageCaption } from "@exp/ui"

import { ComposerSpecimen } from "./composer-specimen.tsx"
import type { StyleguideEntry } from "./types.ts"

// EXP-1293 Threads v2: a thread's CONVERSATION, the REAL `ThreadConversation`
// over `buildSessionThread()` run on the contract's own cases
// (`session-thread.json`, ×4). EXP-1334: the conversation IS the
// dispatcher — no root card; a CHILD's own question / plan card surfaces
// here, THE same card, answered in place. A person's
// bubble right-aligned (`queued` until the device consumed it), the agent's
// text, a child's session card, an event line, a question card answered in
// the card (THE session view's card, `ask-card.tsx`, EXP-1316; case 0's
// options carry descriptions, EXP-1322) and the
// viewer's own queued row with its X (EXP-1317). EXP-1323: the dispatcher
// cases: THE suggested tasks card in every state (open with Start all;
// started = the child's session card; Sent here;
// Filed EXP-9), a child's `Report from …` line.

type Input = Parameters<typeof buildSessionThread>[0]

const noop = (): void => {}

const SHOT = `data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="900"><rect width="1440" height="900" fill="#3f3f46"/></svg>`
)}`

const paragraph = (text: string) => (
  <p className="text-sm leading-relaxed whitespace-pre-wrap text-foreground">{text}</p>
)

const AT = Date.UTC(2026, 9, 13, 19, 56)

const SUGGESTION_ACTIONS = {
  onStart: noop,
  onStartAll: noop,
  onHere: noop,
  onDismiss: noop,
  onFile: noop,
  devices: [{ id: `dev-mac`, name: `Alex's MacBook Pro` }],
  models: [
    { value: `fable`, label: `Fable` },
    { value: `opus`, label: `Opus` },
    { value: `sonnet`, label: `Sonnet` },
    { value: `haiku`, label: `Haiku` },
  ],
}

/** The EXP-1323 dispatcher cases (by name, the fixture may grow). */
const caseIndex = (prefix: string) => fixture.cases.findIndex((entry) => entry.name.startsWith(prefix))

function Case({ index }: { index: number }) {
  const found = fixture.cases[index]
  if (!found) return null
  const view = buildSessionThread(found.input as unknown as Input)
  return (
    <TooltipProvider>
    <ThreadConversation
      rows={view.rows}
      renderText={paragraph}
      attachmentSrc={() => SHOT}
      messageCaption={(row) => {
        const caption = userMessageCaption({ name: `Alex Carter`, at: AT })
        return row.queued ? `${caption} · queued` : caption
      }}
      onOpenSession={() => {}}
      onAnswer={() => {}}
      onDecide={() => {}}
      viewerId="u-dana"
      onUnqueue={() => {}}
      suggestions={SUGGESTION_ACTIONS}
    />
    </TooltipProvider>
  )
}

export const entry: StyleguideEntry = {
  id: `session-thread`,
  section: `special`,
  owner: `EXP-1293`,
  title: `Conversation`,
  blurb: `A thread's Conversation tab (a thread IS a root session, and the Conversation IS that dispatcher: EXP-1334, no card of its own): the ROOT's messages oldest first, with every CHILD's own question / plan card in createdAt order (THE same card, answered in place, straight back to that child). A person's message is the right-aligned bubble (\`<name> · <time>\` under it, \`queued\` until the device consumed it; the viewer's OWN queued row carries the \`Remove from queue\` X with its tooltip, EXP-1317); the agent's reply is markdown at the reading width; a system line is muted. A \`session\` / \`thread_started\` card is THE session card (the Work card's header row) opening that session; a question or plan card is THE session view's card (EXP-1316, \`AskCard\` + \`AskPrompt\`): a question offers its options (label, and its description under it when the option carries one, EXP-1322) and a free-text row (\`Type something.\` opens the inline field), a plan Approve / Reject (Reject opens a note field); an answered card renders closed (the answer, options gone); a subagent is a muted row, a picture the image, an event one muted line (\`Linked EXP-12\`, \`Opened PR #45\`). #EXP-1464 (\`thread-row-action.json\` ×4): a row naming ANOTHER session does something and only then carries the chevron + hover/press: a Needs you session card (not behind a flag, its card not already inline) EXPANDS in place into that session's waiting question / plan card (THE same card, its header's \`Open chat\` opening the session; answered = the row collapses to the quiet \`Answered\`); every other session card, \`Report from\` line or event line about a session (\`… was not picked up\`) OPENS it by inbox-route.json \`sessionTarget\`. Children's other messages never inline. EXP-1323: THE suggested tasks card = the Work card's shell: the header (\`3 suggested tasks\`, \`· 2 started\`; \`Start all\` while more than one is open), then per item the title and text with ×, \`Options ▾\` (THE Menu: Start on… a device, Start with… a model, File as issue, Dismiss), \`Do it here\` and \`Start\` (phones stack the buttons); a started item IS the child's session card, then \`Sent here\` / \`Filed EXP-9\`. A child's end = ONE line \`Report from <session> ›\` opening it (the parent's next message relays the summary). While the root is live the Conversation ends on the raw view's transcript bottom (THE Queue) + status line. No captions, no Show work, no right sidebar. Below it THE composer (EXP-1334: one component for a start, a thread, an opened session and an agent's channel): the card with the "+" (the options: Effort, Ultracode, MCP servers, Computer use, changeable mid-session) and the inline tools (image, file, #, emoji), Send ⇄ Stop; UNDER the card the model pin (a pick sends \`/model <alias>\`), the Plan switch and the context ring (its usage popover on click); \`/\` opens the command menu; placeholder \`Type / for commands\`. Phones: the floating capsule expands into the same card. Model = \`buildSessionThread()\` + \`threadRowAction()\` (fixtures \`session-thread.json\`, \`thread-row-action.json\` ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `ThreadConversation`,
      file: `packages/ui/src/session-thread-view.tsx`,
      note: `page: apps/web/src/routes/t/$teamSlug/threads/$threadId.tsx; model: domain-contract session-tree.ts`,
    },
    desktop: {
      state: `ok`,
      symbol: `ThreadScreenView`,
      file: `apps/desktop/crates/ui/src/thread_screen.rs`,
      note: `model: crates/domain/src/session_tree.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `ThreadScreen`,
      file: `apps/ios/Exponential/UI/Thread/ThreadScreen.swift`,
      note: `model: ExpCore SessionTree.swift`,
    },
    android: {
      state: `ok`,
      symbol: `ThreadScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/thread/ThreadScreen.kt`,
      note: `model: domain/SessionTree.kt`,
    },
  },
  island: () => (
    <div className="flex max-w-2xl flex-col gap-10">
      <Case index={caseIndex(`EXP-1334: the dispatcher`)} />
      <Case index={0} />
      <Case index={1} />
      <div className="grid gap-2">
        <p className="text-xs text-muted-foreground">
          The thread's composer = THE session composer (EXP-1334): the card with "+" (the options) and
          the inline tools, model · Plan · the context ring under it.
        </p>
        <ComposerSpecimen phase="idle" placeholder={fixture.copy.placeholder} />
      </div>
      <div className="grid gap-2">
        <p className="text-xs text-muted-foreground">The same composer in a session while it works.</p>
        <ComposerSpecimen phase="running" placeholder={fixture.copy.placeholder} />
      </div>
      <div className="grid gap-2" data-specimen="quick-replies">
        <p className="text-xs text-muted-foreground">
          EXP-1211: the agent's quick replies (`sessions_suggest` replies) above the composer; a tap
          sends one as the next message, the others stay.
        </p>
        <ComposerSpecimen
          phase="idle"
          placeholder={fixture.copy.placeholder}
          replies={fixture.repliesCases[1].expected as ComposerRepliesModel}
        />
      </div>
      <Case index={caseIndex(`EXP-1323: an ended`)} />
      <Case index={caseIndex(`EXP-1323: a live`)} />
    </div>
  ),
}
