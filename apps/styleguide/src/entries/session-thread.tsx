import fixture from "@exp/domain-contract/fixtures/session-thread.json"
import { buildSessionThread } from "@exp/domain-contract/session-tree"
import { contract } from "@exp/domain-contract"
import {
  Composer,
  ComposerSubmit,
  ComposerTool,
  ContextRing,
  ModelPicker,
  ThreadConversation,
  TooltipProvider,
  conceptIcon,
  userMessageCaption,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1293 Threads v2: a thread's CONVERSATION, the REAL `ThreadConversation`
// over `buildSessionThread()` run on the contract's own cases
// (`session-thread.json`, ×4): the root's session card first, a person's
// bubble right-aligned (`queued` until the device consumed it), the agent's
// text, a child's session card, an event line, a question card answered in
// the card (THE session view's card, `ask-card.tsx`, EXP-1316; case 0's
// options carry descriptions, EXP-1322) and the
// viewer's own queued row with its X (EXP-1317).

type Input = Parameters<typeof buildSessionThread>[0]

const UiAddIcon = conceptIcon(`ui-add`)
const noop = (): void => {}

/** THE model choices (contract `codingModel`, all four, no Default row). */
const MODELS = contract.codingModel.values.map((value) => ({
  value,
  label: value.charAt(0).toUpperCase() + value.slice(1),
}))

/** THE session composer (the app's `SessionComposer`): "+" · the model pin
 *  · (a plan-mode child's read-out) … the context ring · Send ⇄ Stop. */
function SessionComposerSpecimen({
  model,
  percent,
  tone,
  plan = false,
  busy = false,
}: {
  model: string
  percent: number
  tone: `normal` | `warning` | `danger`
  plan?: boolean
  busy?: boolean
}) {
  return (
    <Composer
      tools={
        <>
          <ComposerTool aria-label="Add file">
            <UiAddIcon className="size-4" />
          </ComposerTool>
          <span className="ml-1 flex items-center text-xs text-muted-foreground">
            <ModelPicker
              value={model}
              models={MODELS}
              onChange={noop}
            />
          </span>
          {plan && <span className="ml-2 text-xs text-sky-400">Plan</span>}
        </>
      }
      submit={
        <>
          <ContextRing percent={percent} tone={tone} />
          <ComposerSubmit stop={busy} />
        </>
      }
    >
      <div className="min-h-20 px-3 pt-3 text-sm text-muted-foreground">
        {fixture.copy.placeholder}
      </div>
    </Composer>
  )
}

const SHOT = `data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="900"><rect width="1440" height="900" fill="#3f3f46"/></svg>`
)}`

const paragraph = (text: string) => (
  <p className="text-sm leading-relaxed whitespace-pre-wrap text-foreground">{text}</p>
)

const AT = Date.UTC(2026, 9, 13, 19, 56)

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
    />
    </TooltipProvider>
  )
}

export const entry: StyleguideEntry = {
  id: `session-thread`,
  section: `special`,
  owner: `EXP-1293`,
  title: `Conversation`,
  blurb: `A thread's Conversation tab (a thread IS a root session): the root's own session card first, then the ROOT's messages oldest first. A person's message is the right-aligned bubble (\`<name> · <time>\` under it, \`queued\` until the device consumed it; the viewer's OWN queued row carries the \`Remove from queue\` X with its tooltip, EXP-1317); the agent's reply is markdown at the reading width; a system line is muted. A \`session\` / \`thread_started\` card is THE session card (the Work card's header row) opening that session; a question or plan card is THE session view's card (EXP-1316, \`AskCard\` + \`AskPrompt\`): a question offers its options (label, and its description under it when the option carries one, EXP-1322) and a free-text row (\`Type something.\` opens the inline field), a plan Approve / Reject (Reject opens a note field); an answered card renders closed (the answer, options gone); a subagent is a muted row, a picture the image, an event one muted line (\`Linked EXP-12\`, \`Opened PR #45\`). Children's own messages never inline. No captions, no Show work, no right sidebar. Below it THE session composer, the same box on the thread (targeting the root), an opened session and an agent's channel: the "+", the model pin (THE model picker inline, all four models, no Default; a pick sends \`/model <alias>\`), the plan read-out only on a plan-mode session that is not a dispatcher, then the context ring (its usage popover on click) and Send ⇄ Stop; \`/\` opens the command menu; placeholder \`Reply…\`. Phones: the floating capsule expands into the same card. Model = \`buildSessionThread()\` (fixture \`session-thread.json\` ×4).`,
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
      <Case index={0} />
      <Case index={1} />
      <div className="grid gap-2">
        <p className="text-xs text-muted-foreground">
          The thread's composer: a dispatcher root on its thread model, no plan read-out.
        </p>
        <SessionComposerSpecimen model="haiku" percent={32} tone="normal" />
      </div>
      <div className="grid gap-2">
        <p className="text-xs text-muted-foreground">
          The session view of a plan-mode child while it works.
        </p>
        <SessionComposerSpecimen model="opus" percent={78} tone="warning" plan busy />
      </div>
    </div>
  ),
}
