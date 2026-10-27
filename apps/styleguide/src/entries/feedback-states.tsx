import feedbackStates from "@exp/domain-contract/fixtures/feedback-states.json"
import { FeedbackStatesSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1275 + EXP-1271 + FEED-79 (EXP-1259 D8-D10): THE feedback states, the
// six kinds of `feedback-states.json` side by side (draft 6), drawn by
// `@exp/ui` `FeedbackStatesSpecimen` (every specimen the REAL part at rest),
// the same board the web app's `feedback-states` capture photographs.

const { pending, starting } = feedbackStates

export const entry: StyleguideEntry = {
  id: `feedback-states`,
  section: `general`,
  owner: `EXP-1275`,
  title: `Feedback states`,
  blurb: `Six kinds, nothing else draws a wait or an error (fixture \`feedback-states.json\` ×4). 1 Inline: a ${feedbackStates.inline.spinner}px spinner + a progressive word in a band or caption (Saving, then Saved); the list-head spinner is the same glyph. 2 Pending: the button's icon slot turns into the spinner, its label into the progressive word (Starting, Connecting), its width never shrinks, nothing shows before ${pending.delayMs} ms. 3 Placeholder: skeleton rows in the real row geometry, one slow pulse, static under reduce motion; replaces every "Loading…". 4 The mark: the disc's three stripes sweep in turn (${feedbackStates.mark.loopMs / 1000} s loop, vector ×4, the static logo under reduce motion); "Starting on <device>" in the composer until the session lands, ${starting.timeoutMs / 1000} s then a toast with Retry. 5 Blocking: a window backdrop + the mark + one line, only for switching team or account and signing out. 6 Errors: a failed action is a toast (\`Couldn't <verb> <thing>\`, Retry when safe; a merge the check gate refused offers Merge anyway instead); invalid input stays under its field.`,
  status: {
    web: {
      state: `ok`,
      symbol: `Spinner / InlineStatus / Button pending / SkeletonRows / FeedbackMark / BlockingOverlay`,
      file: `packages/ui/src/feedback.tsx`,
      note: `pending = button.tsx, placeholder = skeleton.tsx, the spinner = ui-react primitives/spinner.tsx, this board = feedback-specimen.tsx`,
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
  island: () => <FeedbackStatesSpecimen />,
}
