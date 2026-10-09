import {
  RunStatusRow,
  SessionThreadView,
  sessionThread,
  sessionTurns,
  userMessageCaption,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1175: the Run face's default body — the REAL `SessionThreadView` over a
// static `results` blob run through the shared `sessionThread` rule.
// EXP-1245: and the OWNER's form, the same view over `sessionTurns` (two
// turns + the person's bubble), one REAL `RunStatusRow` per turn with the
// app's `turnRowCaption` words passed literal.

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

const T0 = Date.UTC(2026, 9, 6, 19, 16)

const TURNS = sessionTurns(
  [
    {
      topic: `Summary`,
      text: `The iOS New issue page showed Back and a × together. The × is gone, so the trailing cluster is just Create.`,
      at: T0 + 300_000,
    },
    {
      topic: `Reviews: linked PR shown as not linked`,
      text: `Reviews matched PRs by branch name, so a batch-head PR never found its issue. It now resolves by the exact pr_url.`,
      at: T0 + 1_800_000,
    },
  ],
  [
    { kind: `turn`, state: `started`, at: T0 },
    { kind: `turn`, state: `ended`, at: T0 + 437_000 },
    {
      kind: `user_message`,
      at: T0 + 1_440_000,
      text: `why is this PR not connected to the issue? file a new issue and stack on top of this one`,
      images: [SHOT],
    },
    { kind: `turn`, state: `started`, at: T0 + 1_441_000 },
  ]
).turns

const TURN_ROWS = [
  { caption: `Done on macbook · 7m 17s`, markState: `ended` as const, toolLine: null },
  { caption: `Building on macbook · 7m 02s`, markState: `working` as const, toolLine: `Bash grep` },
]

const paragraph = (text: string) => (
  <p className="text-sm leading-relaxed text-foreground">{text}</p>
)

export const entry: StyleguideEntry = {
  id: `session-thread`,
  section: `special`,
  owner: `EXP-1175`,
  title: `Results thread`,
  blurb: `The thread under the status row: the run's published results in PUBLISH order (\`sessionThread\`): a topic's first text under its muted caption, a picture where it was published, and the Summary text LAST as the agent's reply. Pending plan and question cards follow it, so a question never hides. Show work swaps the whole thread for the transcript in place (fixture \`session-results.json\` \`thread\`). EXP-1245: for the run's OWNER the thread is a conversation of TURNS off the relay feed (\`sessionTurns\`, fixture \`turns\`): the person's message as a right-aligned bubble (max 70%, an image thumb, \`<name> · <time> · from <device>\` under it), then that turn's own status row (\`Done on <device> · <turn time>\` once settled, the live row timed from the turn's start, fixture \`run-row.json\` \`turnCaptions\`), then the results the server stamped inside it. Teammates and an offline host keep the single-row thread (first island).`,
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
    <div className="flex flex-col gap-8">
      <SessionThreadView
        thread={THREAD}
        attachmentSrc={() => SHOT}
        renderText={paragraph}
        renderReply={paragraph}
      />
      <SessionThreadView
        turns={TURNS}
        renderStatusRow={(_turn, index) => {
          const row = TURN_ROWS[index]
          return row ? (
            <RunStatusRow
              agent="claude"
              markState={row.markState}
              caption={row.caption}
              tone="muted"
              toolLine={row.toolLine}
              toggleLabel="Show work"
              onToggle={() => {}}
            />
          ) : null
        }}
        messageCaption={(message) =>
          userMessageCaption({ name: `Danny`, at: message.at, device: `mint` })
        }
        attachmentSrc={() => SHOT}
        renderText={paragraph}
        renderReply={paragraph}
      />
    </div>
  ),
}
