import { MentionPill } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// Thread e4a04d22 (owner 2026-10-24: "the mentions should also get a proper
// ui everywhere"): THE round `@member` pill the comments paint, now in every
// chat's text too. A chat line with the REAL `@exp/ui` pill for a member
// beside an unresolved `@x@y.com` that stays the written text.

export const entry: StyleguideEntry = {
  id: `mention-pill`,
  section: `general`,
  owner: `EXP-1373`,
  title: `Mention pill`,
  blurb: `The one pill for an @<email> mention, identical in issue descriptions, comments and every chat (thread, agent and run transcripts, plain and markdown text alike): a fully round capsule (the accent fill, the border hairline, 13 px medium, never wrapping) reading "@" + the member's display name, the email its tooltip. Only an email that resolves to a member of the team the viewer can see becomes a pill; anything else (an outsider, a typo, a former member) stays the plain written text. The document text keeps the bare @<email> token, so the markdown round-trip never changes.`,
  status: {
    web: {
      state: `ok`,
      symbol: `MentionPill / .mention-pill`,
      file: `packages/ui/src/mention-pill.tsx`,
      note: `chats: IssueRefText (apps/web/src/components/feed-text.tsx); editors: the decoration in apps/web/src/lib/mention-pill-extension.ts`,
    },
    desktop: {
      state: `ok`,
      symbol: `MarkdownView (RefResolver.member_name)`,
      file: `apps/desktop/crates/ui/src/markdown/editor.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `MentionRefs.decorateForDisplay`,
      file: `apps/ios/ExpUI/Sources/MentionRefs.swift`,
    },
    android: {
      state: `ok`,
      symbol: `MentionResolver (DecorationStyle.MentionPill)`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/markdown/Mentions.kt`,
    },
  },
  island: () => (
    <div className="grid max-w-md gap-2 text-sm text-foreground">
      <p>
        Ready for review, <MentionPill name="Ada Lovelace" email="ada@example.com" />
      </p>
      <p>cc @x@y.com</p>
    </div>
  ),
}
