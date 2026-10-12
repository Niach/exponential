import { AgentBrandMark, CommentCallCard, IssueChip } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1241 (EXP-1259 D14): the comment an `exponential_comments_create`
// call posted, as the run thread draws it — the REAL `CommentCallCard`
// ("Commented on <issue chip>" over the comment in the timeline's look). The
// app renders the body through its markdown editor; the island passes plain
// paragraphs.

const BODY = (
  <div className="space-y-2">
    <p className="font-semibold">Status 2026-10-08, evening</p>
    <p>
      #VAPP-98 (renderer hardening) is open as PR #1007; the host API run
      #VAPP-91 is not based on the new contract yet, so I sent it a heads-up.
    </p>
    <p>
      Next: the conformance fixtures for the Swift and Compose renderers, then
      the docs pass.
    </p>
  </div>
)

export const entry: StyleguideEntry = {
  id: `comment-call-card`,
  section: `special`,
  owner: `EXP-1241`,
  title: `Comment call card`,
  blurb: `A settled \`exponential_comments_create\` call renders the REAL comment, never the tool line: our mark and \`Commented on\` over the issue chip, then the comment in the timeline's comment look (the agent's mark, the author, \`via MCP · <time>\`), the markdown body clamped to three lines behind \`Show more\`. The body, author and time are the synced comment row's (the result preview names its id); until it syncs, the preview's text, the session's owner and the settle's time. Clicking the comment opens the issue at \`?comment=<id>\`: the timeline scrolls to it and washes it for 1.5 s (reduced motion: only the scroll). A running or failed call keeps its ordinary tool row.`,
  status: {
    web: {
      state: `ok`,
      symbol: `CommentCallCard`,
      file: `packages/ui/src/comment-call-card.tsx`,
      note: `binding: apps/web/src/components/agent-comment-card.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `thread_comment_card`,
      file: `apps/desktop/crates/ui/src/comments.rs`,
      note: `steer_viewer::render_comment_call; styleguide/entries/comment_card.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `CommentCallCardView`,
      file: `apps/ios/ExpUI/Sources/CommentCallCardView.swift`,
      note: `row: CommentCallRow in UI/Session/AgentWorkflowViews.swift`,
    },
    android: {
      state: `ok`,
      symbol: `CommentCallCard`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/session/CommentCallCard.kt`,
      note: `tap: ui/issue/CommentFocus.kt`,
    },
  },
  island: () => (
    <div className="max-w-2xl text-sm">
      <CommentCallCard
        issue={
          <IssueChip
            identifier="VAPP-84"
            title="Exponential UI: an open generative-UI SDK"
            status={{ icon: `circle-dashed`, colorClass: `text-muted-foreground` }}
            onClick={() => {}}
          />
        }
        author="Danny"
        mark={<AgentBrandMark agent="claude" className="size-3.5 shrink-0" />}
        caption="via MCP · 2 minutes ago"
        body={BODY}
        onOpen={() => {}}
      />
    </div>
  ),
}
