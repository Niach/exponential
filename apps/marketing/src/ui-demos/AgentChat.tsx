/* The home hero's screen (NEW-15): a team agent's chat on the phone, from the
   pieces the agent channel draws (apps/web components/thread/agent-channel.tsx,
   bubbles layout): THE phone detail header (back · agent disc + `Agent` · …),
   `ThreadConversation` with the person's message, the agent's bubble, its
   Exponential UI surface (one card message, `exponential_session_self show`),
   the person's press on a `reply` button (chats.json `surfaceCard.replyEvent`:
   its `context.text` posts as the next message) and the session card of
   what that started. Messages and cards ARRIVE whole, as in the app: the live
   frames hand `ThreadConversation` `arrived`, so the bubbles rise and the
   surface plays its own enter (chats.json ENTER); while the agent works the
   transcript ends on the run view's status line (chats.json WORKING,
   apps/web agent-session.tsx WorkingIndicatorRow). `scene(step)` is the whole
   film: the prerendered island shows the LAST frame (no `step`, nothing
   animates), the page replays every frame. The header is the app's own
   (MobileDetailHeader + AgentHeaderTitle, the agent route's menu drawn as its
   round ghost button); the composer is the agent route's SessionComposer
   (apps/web components/session-composer.tsx): THE Composer card, the field
   reading `Message <name>` (or the draft being typed), then "+" · the model
   pin (the claude default, contract `deviceAgentDefaults.model` = fable) ·
   Plan + its switch, and the round Send (Stop while the agent works). */
import type { ComponentProps, ReactNode } from "react"
import {
  AgentHeaderTitle,
  AgentWorkingMark,
  Button,
  Composer,
  ComposerFrame,
  ComposerSubmit,
  ComposerTool,
  HEADER_BUTTON_CLASS,
  InlinePickerRow,
  Label,
  MobileDetailHeader,
  ModelPicker,
  Switch,
  Textarea,
  ThreadConversation,
  TooltipProvider,
  conceptIcon,
} from "@exp/ui"

type Rows = ComponentProps<typeof ThreadConversation>[`rows`][number][]
type Node = {
  id: string
  component: string
  props?: Record<string, unknown>
  style?: Record<string, unknown>
  on?: Record<string, unknown>
  children?: Node[]
}

const MoreIcon = conceptIcon(`ui-more`)
/** The "+" glyph (apps/web launch-dialog/composer-plus-menu.tsx `UiAddIcon`). */
const UiAddIcon = conceptIcon(`ui-add`)
/** composer-menu.json `plusLabel`. */
const PLUS_LABEL = `Add`
/** The model pin before the relay says: contract `deviceAgentDefaults.model`
 *  (apps/web session-composer.tsx `fallbackModel` → `defaultModelFor`).
 *  The choices = `modelChoices(\`claude\`)` (launch-options-pane.tsx: contract
 *  `codingModel.values`, capitalised by `modelLabel`); one option would
 *  collapse the pin to a bare word (combobox.tsx), so all four are listed. */
const MODEL = `fable`
const MODELS = [`fable`, `opus`, `sonnet`, `haiku`].map((value) => ({ value, label: value.charAt(0).toUpperCase() + value.slice(1) }))

const AGENT = { name: `Sloppy`, icon: `bot`, color: `#f97316` }
export const PROMPT = `How did the week go?`
const ANSWER = `Good week. One thing is stuck:`
const TAP = `Fix the blocker`
/** chats.json `surfaceCard.replyEvent` (marketing does not declare
 *  @exp/domain-contract, so the word is copied). */
const REPLY_EVENT = `reply`
/** The run view's working line: `{verb}… ({duration})` (apps/web
 *  lib/working-caption.ts; the verb is contract `steerWorking.verbs[0]`). */
const WORKING = `Thinking… (2s)`

const reply = (text: string) => ({ press: { event: { name: REPLY_EVENT, context: { text } } } })

/* The answer's surface, in the nested authoring form (as fixtures/demo-surface.json):
   flat on purpose, ONE chart, two rows and the two actions, no cards inside the card.
   Both buttons are `reply` presses (chats.json: every other event is ignored). */
const SURFACE: Node = {
  id: `week`,
  component: `Box`,
  style: { display: `flex`, flexDirection: `column`, gap: `$spacing.lg` },
  children: [
    {
      id: `week-chart`,
      component: `Chart`,
      props: {
        kind: `bar`,
        height: 132,
        showLegend: false,
        showGrid: false,
        categories: [`Mon`, `Tue`, `Wed`, `Thu`, `Fri`],
        series: [{ name: `Shipped`, values: [2, 3, 1, 4, 2] }],
      },
    },
    { id: `week-shipped`, component: `Row`, props: { title: `Shipped`, value: `12` } },
    { id: `week-blocked`, component: `Row`, props: { title: `Blocked`, identifier: `APP-9`, subtitle: `Uploads fail on Android`, value: `2 days` } },
    {
      id: `week-actions`,
      component: `Stack`,
      props: { direction: `horizontal`, gap: `sm`, justify: `end` },
      children: [
        { id: `week-board`, component: `Button`, props: { label: `Open board`, variant: `outline` }, on: reply(`Open the board`) },
        { id: `week-fix`, component: `Button`, props: { label: TAP, variant: `default` }, on: reply(TAP) },
      ],
    },
  ],
}

/* The film, one arrival per frame. */
const ASKED = 1
const ASK_WORKING = 2
const ANSWERED = 3
const SURFACED = 4
const TAPPED = 5
const TAP_WORKING = 6
export const LAST_STEP = 7

/** Milliseconds a frame stays. */
export function frameMs(step: number): number {
  if (step === 0) return 700
  if (step === ASK_WORKING || step === TAP_WORKING) return 1100
  if (step === SURFACED) return 2600
  return 900
}

export function scene(step: number): { rows: Rows; working: boolean; draft: string } {
  const rows: Rows = []
  if (step >= ASKED) rows.push({ type: `message`, id: `ask`, author: `user`, authorId: `you`, body: PROMPT, queued: false, authorSessionId: null, xref: null })
  if (step >= ANSWERED) rows.push({ type: `message`, id: `answer`, author: `agent`, authorId: null, body: ANSWER, queued: false, authorSessionId: null, xref: null })
  if (step >= SURFACED) rows.push({ type: `surface`, id: `week`, sessionId: `sloppy`, title: `Shipped this week`, surface: { root: SURFACE, data: {} } })
  if (step >= TAPPED) rows.push({ type: `message`, id: `tap`, author: `user`, authorId: `you`, body: TAP, queued: false, authorSessionId: null, xref: null })
  if (step >= LAST_STEP) {
    rows.push({
      type: `session`,
      id: `fix`,
      /* session-card.json `sessionCard` of a working child whose title is
         its one issue (session-title.json `titles.issue`). */
      card: {
        sessionId: `fix`,
        icon: `coding-running`,
        name: `#APP-9 Uploads fail on Android`,
        kindWord: `Working`,
        state: `working`,
        tone: `foreground`,
        elapsed: `8s`,
        running: true,
        testId: `session-card`,
      },
    })
  }
  const working = (step >= ASK_WORKING && step < SURFACED) || step === TAP_WORKING
  /* The first frame holds the typed prompt in the field; the next sends it. */
  const draft = step < ASKED ? PROMPT : ``
  return { rows, working, draft }
}

/* The agent route's composer on a phone (apps/web routes/t/$teamSlug/agents/
   $agentId.tsx L204-226: `shrink-0 px-4 pt-2 pb-4` around SessionComposer),
   copied from apps/web components/session-composer.tsx L408-575 with the
   app-only parts drawn as their markup: ComposerOptionsMenu's trigger
   (composer-plus-menu.tsx L290-298), no device pin (SessionDevicePin renders
   nothing until a move is offered), MentionTextarea's wrapper
   (mention-textarea.tsx L416) around the same Textarea. */
function AgentComposerCard({ draft, busy }: { draft: string; busy: boolean }) {
  const empty = draft.trim().length === 0
  const showStop = busy && empty
  return (
    <ComposerFrame className="mx-auto w-full max-w-4xl">
      <InlinePickerRow slotClassName="pt-1.5 text-sm text-foreground">
        <Composer
          tools={
            <>
              <ComposerTool aria-label={PLUS_LABEL} title={PLUS_LABEL} tabIndex={-1}>
                <UiAddIcon />
              </ComposerTool>
              <div className="flex min-w-0 items-center gap-x-2.5 pl-1 text-xs text-muted-foreground">
                <span>
                  <ModelPicker value={MODEL} models={MODELS} onChange={() => {}} />
                </span>
                <Label className="cursor-pointer gap-1.5 font-normal">
                  <span>Plan</span>
                  <Switch size="sm" checked={false} tabIndex={-1} aria-label="Plan mode" />
                </Label>
              </div>
            </>
          }
          submit={<ComposerSubmit stop={showStop} tabIndex={-1} disabled={!showStop && empty} />}
        >
          <div className="relative">
            <div className="relative flex-1">
              <Textarea
                readOnly
                tabIndex={-1}
                value={draft}
                placeholder={`Message ${AGENT.name}`}
                className="min-h-20 resize-none border-0 bg-transparent px-3 pt-3 shadow-none focus-visible:ring-0"
              />
            </div>
          </div>
        </Composer>
      </InlinePickerRow>
    </ComposerFrame>
  )
}

/* The app's thread text on its plain path (apps/web feed-text.tsx), at the
   bubbles' measured line height; the colour stays the bubble's own. */
const paragraph = (text: string): ReactNode => <div className="leading-relaxed whitespace-pre-wrap break-words">{text}</div>

const live = () => true
const settled = () => false

export function AgentChat({ step }: { step?: number }) {
  const { rows, working, draft } = scene(step ?? LAST_STEP)
  return (
    <TooltipProvider>
      <div className="relative flex h-[820px] flex-col bg-background pt-[54px]">
        <MobileDetailHeader
          title={<AgentHeaderTitle name={AGENT.name} icon={AGENT.icon} color={AGENT.color} />}
          onBack={() => {}}
          menu={
            <Button variant="ghost" size="icon" className={HEADER_BUTTON_CLASS} aria-label="More">
              <MoreIcon className="size-4" />
            </Button>
          }
        />
        <div className="flex min-h-0 flex-1 flex-col justify-end gap-2.5 overflow-hidden px-4 pb-8">
          <ThreadConversation
            rows={rows}
            layout="bubbles"
            renderText={paragraph}
            attachmentSrc={() => ``}
            messageTime={() => `09:41`}
            onSurfaceReply={() => {}}
            onOpenSession={() => {}}
            arrived={step === undefined ? settled : live}
          />
          {working && (
            <div className="flex items-center gap-2 text-[length:var(--transcript-tool-size)] leading-[var(--transcript-tool-line-height)]">
              <AgentWorkingMark agent="claude" />
              <span className="min-w-0 truncate text-muted-foreground">{WORKING}</span>
            </div>
          )}
        </div>
        <div className="shrink-0 px-4 pt-2 pb-4">
          <AgentComposerCard draft={draft} busy={working} />
        </div>
      </div>
    </TooltipProvider>
  )
}
