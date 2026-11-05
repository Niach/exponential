import { NeedsYouSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #FEED-191 NEEDS YOU (fixtures/needs-you.json): the list's rows (`cases[1]`)
// and the phones' entry row (and the app's `?dev=needs-you` capture).

export const entry: StyleguideEntry = {
  id: `needs-you`,
  section: `special`,
  owner: `FEED-191`,
  title: `Needs you`,
  blurb: `Every plan and question waiting on the viewer and every PR ready to merge (fixture \`needs-you.json\` ×4; Exponential-specific, not eUI): one reading column of THE inbox page's ListRow, newest first: the glyph in the needs-you tone, the title with its age muted at the end, then the chat · meta muted. A press opens the chat on the right tab. None = THE EmptyState. Wide: a sidebar entry right after Agent with a count Badge, gone at 0. Phones: the first row of Chats (the row above the list here), pushing the list.`,
  status: {
    web: {
      state: `ok`,
      symbol: `NeedsYouList / NeedsYouEntryRow`,
      file: `packages/ui/src/needs-you.tsx`,
      note: `host: routes/t/$teamSlug/needs-you.tsx (useNeedsYou), the sidebar entry, the phones' Chats list`,
    },
    desktop: { state: `n/a`, note: `the desktop lane (model: domain::needs_you)` },
    ios: { state: `n/a`, note: `the native lane (model: ExpCore NeedsYou.swift)` },
    android: { state: `n/a`, note: `the native lane (model: domain/NeedsYou.kt)` },
  },
  island: () => <NeedsYouSpecimen />,
}
