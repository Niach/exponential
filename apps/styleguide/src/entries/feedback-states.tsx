import type { ReactNode } from "react"
import feedbackStates from "@exp/domain-contract/fixtures/feedback-states.json"
import {
  BlockingSurface,
  Button,
  FeedbackMark,
  GlassSectionHeader,
  InlineStatus,
  Input,
  SkeletonRows,
  ToastSpecimen,
  blockingLabel,
  conceptIcon,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1275 + EXP-1271 + FEED-79 (EXP-1259 D8-D10): THE feedback states, the
// six kinds of `feedback-states.json` side by side (draft 6). Every specimen
// is the REAL `@exp/ui` part at rest: the pending Button with a zero delay
// (the live one waits `pending.delayMs`), the mark frozen at four phases of
// its loop (`phase`), the blocking surface contained in its card instead of
// the window, the error toast as `ToastSpecimen`.

const { pending, starting, errors } = feedbackStates
const UiSubmitIcon = conceptIcon(`ui-submit`)
const MARK_PHASES = [0.08, 0.2, 0.32, 0.5]

function Kind({ index, title, children }: { index: number; title: string; children: ReactNode }) {
  return (
    <div className="grid content-start gap-3 rounded-xl border border-border p-4">
      <p className="text-sm font-medium">{`${index} · ${title}`}</p>
      {children}
    </div>
  )
}

function Band({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex h-10 items-center justify-between rounded-md bg-muted/40 px-3 text-sm">
      <span>{label}</span>
      {children}
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `feedback-states`,
  section: `general`,
  owner: `EXP-1275`,
  title: `Feedback states`,
  blurb: `Six kinds, nothing else draws a wait or an error (fixture \`feedback-states.json\` ×4). 1 Inline: a ${feedbackStates.inline.spinner}px spinner + a progressive word in a band or caption (Saving, then Saved); the list-head spinner is the same glyph. 2 Pending: the button's icon slot turns into the spinner, its label into the progressive word (Starting, Connecting), its width never shrinks, nothing shows before ${pending.delayMs} ms. 3 Placeholder: skeleton rows in the real row geometry, one slow pulse, static under reduce motion; replaces every "Loading…". 4 The mark: the disc's three stripes sweep in turn (${feedbackStates.mark.loopMs / 1000} s loop, vector ×4, the static logo under reduce motion); "Starting on <device>" in the composer until the run lands, ${starting.timeoutMs / 1000} s then a toast with Retry. 5 Blocking: a window backdrop + the mark + one line, only for switching team or account and signing out. 6 Errors: a failed action is a toast (\`Couldn't <verb> <thing>\`, Retry when safe); invalid input stays under its field.`,
  status: {
    web: {
      state: `ok`,
      symbol: `Spinner / InlineStatus / Button pending / SkeletonRows / FeedbackMark / BlockingOverlay`,
      file: `packages/ui/src/feedback.tsx`,
      note: `pending = button.tsx, placeholder = skeleton.tsx, the spinner = ui-react primitives/spinner.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `feedback::{spinner, inline_status, Pending, skeleton_rows, mark}`,
      file: `apps/desktop/crates/ui/src/feedback.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `InlineSpinner / InlineStatus / FeedbackMark`,
      file: `apps/ios/ExpUI/Sources/Feedback/Spinner.swift`,
      note: `the mark = Feedback/FeedbackMark.swift`,
    },
    android: {
      state: `leftover`,
      symbol: `markStripeSpan`,
      file: `apps/android/app/src/main/java/com/exponential/app/domain/FeedbackMark.kt`,
      note: `the mark's timeline model only; the composables land with the Android feedback lane`,
    },
  },
  island: () => (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(20rem,1fr))] gap-4">
      <Kind index={1} title="Inline">
        <Band label="Team prompt">
          <InlineStatus state="saving" />
        </Band>
        <Band label="Team prompt">
          <InlineStatus state="saved" />
        </Band>
        <GlassSectionHeader label="MCP servers" count={3} loading />
      </Kind>
      <Kind index={2} title="Pending action">
        <div className="flex flex-wrap items-center gap-2">
          <Button>
            <UiSubmitIcon />
            Start
          </Button>
          <Button pending pendingDelayMs={0} pendingLabel={pending.words.start}>
            <UiSubmitIcon />
            Start
          </Button>
          <Button variant="outline" pending pendingDelayMs={0} pendingLabel={pending.words.connect}>
            Connect
          </Button>
        </div>
      </Kind>
      <Kind index={3} title="Placeholder">
        <SkeletonRows />
      </Kind>
      <Kind index={4} title="The mark">
        <div className="flex items-center gap-4">
          {MARK_PHASES.map((phase) => (
            <FeedbackMark key={phase} size={feedbackStates.mark.sizes.styleguide} phase={phase} />
          ))}
        </div>
        <div className="flex items-center gap-2 rounded-xl border border-border px-3 py-2.5 text-sm">
          <FeedbackMark size={feedbackStates.mark.sizes.inline} phase={0.3} />
          <span className="flex-1">{starting.label.replace(`{device}`, `macbook`)}</span>
          <span className="text-xs tabular-nums text-muted-foreground">1.4 s</span>
        </div>
      </Kind>
      <Kind index={5} title="Blocking">
        <div className="relative h-40 overflow-hidden rounded-lg border border-border">
          <BlockingSurface contained label={blockingLabel(`switchTeam`, `Personal`)} />
        </div>
      </Kind>
      <Kind index={6} title="Errors">
        <ToastSpecimen
          kind="error"
          title={errors.cases[0]!.title!}
          description="Network error"
          action={errors.retry}
        />
        <div className="grid gap-1.5">
          <Input aria-invalid defaultValue="dennis@" />
          <p className="text-[13px] text-destructive">{errors.cases[4]!.message}</p>
        </div>
      </Kind>
    </div>
  ),
}
