import { SessionThreadView, sessionThread } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1175: the Run face's default body — the REAL `SessionThreadView` over a
// static `results` blob run through the shared `sessionThread` rule.

const SHOT = `data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="900"><rect width="1440" height="900" fill="#3f3f46"/></svg>`
)}`

const THREAD = sessionThread([
  {
    topic: `Summary`,
    text: `The Run face now opens on the results thread; Show work brings the transcript back in place.`,
  },
  {
    topic: `Status row`,
    text: `One row over the thread: the run mark, the caption, the last tool line and Show work.`,
  },
  {
    topic: `Status row`,
    label: `web`,
    attachmentId: `shot-1`,
    width: 1440,
    height: 900,
    inline: true,
    caption: `The thread on md+`,
  },
  {
    topic: `Preference`,
    text: `Show work is remembered per user in this browser.`,
  },
])

const paragraph = (text: string) => (
  <p className="text-sm leading-relaxed text-foreground">{text}</p>
)

export const entry: StyleguideEntry = {
  id: `session-thread`,
  section: `special`,
  owner: `EXP-1175`,
  title: `Results thread`,
  blurb: `The thread under the status row: the run's published results in PUBLISH order (\`sessionThread\`): a topic's first text under its muted caption, a picture where it was published, and the Summary text LAST as the agent's reply. Pending plan and question cards follow it, so a question never hides. Show work swaps the whole thread for the transcript in place (fixture \`session-results.json\` \`thread\`).`,
  status: {
    web: {
      state: `ok`,
      symbol: `SessionThreadView`,
      file: `packages/ui/src/session-thread-view.tsx`,
      note: `rule: session-results.ts sessionThread, fixture session-results.json thread`,
    },
    desktop: {
      state: `ok`,
      symbol: `steer_viewer::render_thread`,
      file: `apps/desktop/crates/ui/src/steer_viewer.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `AgentSessionView.threadList`,
      file: `apps/ios/Exponential/UI/Session/AgentSessionView.swift`,
    },
    android: {
      state: `ok`,
      symbol: `RunThread`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/session/AgentSessionScreen.kt`,
    },
  },
  island: () => (
    <SessionThreadView
      thread={THREAD}
      attachmentSrc={() => SHOT}
      renderText={paragraph}
      renderReply={paragraph}
    />
  ),
}
