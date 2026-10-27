import chats from "@exp/domain-contract/fixtures/chats.json"
import { ChatSelectionSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1388 (fixtures/chats.json SELECTION + ARCHIVE ×4): the Chats list in
// selection mode, `@exp/ui` `ChatSelectionSpecimen` (the real ChatList with
// the Archived entry, two rows selected, over THE bulk bar capsule); the
// web shot is the real Chats page (`openChatSelection`), beside
// board-bulk-edit.

const COPY = chats.copy

export const entry: StyleguideEntry = {
  id: `chat-selection`,
  section: `special`,
  owner: `EXP-1388`,
  title: `Chat selection`,
  blurb: `The Chats list's MULTI-SELECTION ×4. Touch: a long-press on a chat row enters it with that row (no row menu on touch). Pointers: the issue rows' Checkbox column (on hover, always while selecting), the row's right-click menu starting with "${COPY.selection.select}", cmd/ctrl-click toggles, shift-click selects the range from the last clicked row, cmd/ctrl+A selects all. Pointers keep the issue list's bulk selection (board-bulk-edit): the Checkbox square and the issue row's selected look. Touch (owner on #1286): NOTHING shifts (no control column), a selected row is a full-width fillActive tint and wears the SELECT BADGE, THE checkbox look (owner 2026-10-27: every check is square): an 18px primary square, 4px corner, the Checkbox's ui-check, on its disc's bottom-right corner ringed in the row's ground. Every form factor: a selected row's disc steps one grey notch up (fillActive over muted). A tap toggles and nothing opens (the issue list's long-press selects the same way on every touch platform); the last deselect, the close, Escape or back leave it. THE bar = the issue list's bulk action bar (the same capsule, placement and phone chrome swap): close, the count ("${COPY.selection.count.replace(`{n}`, `2`)}", as on the issue bar), then "${COPY.selection.markRead}" (any unread) or "${COPY.selection.markUnread}", "${COPY.selection.archive}" (the list) or "${COPY.selection.unarchive}" (the Archived page), "${COPY.selection.stop}" while a chat is live, "${COPY.selection.delete}" when every chat may be deleted (confirm "${COPY.selection.confirmDeleteMany.replace(`{n}`, `2`)}"). A failure toasts "${COPY.toasts.selectionFailed}". Archive is per viewer; while chats are archived (filter All) the list's first row is the "${COPY.archived.entry}" entry (the plain ui-archive glyph centred in the discs' column, no disc; the count in the time slot; it never moves) opening the Archived page (empty: "${COPY.archived.empty}").`,
  status: {
    web: {
      state: `ok`,
      symbol: `ChatSelectionSpecimen / ChatList / ChatSelectBadge (phones) / ChatLeadDisc selected / ChatArchivedRow / BulkBarCapsule + the Checkbox recipe (wide)`,
      file: `packages/ui/src/chat-specimens.tsx`,
      note: `routes/t/$teamSlug/threads/index.tsx + components/thread/chat-selection-bar.tsx (?view=archived)`,
    },
    desktop: {
      state: `ok`,
      symbol: `chat_row`,
      file: `apps/desktop/crates/ui/src/chat_list.rs`,
      note: `controls::checkbox (the issue list's select cell), the stronger selected disc + the bulk bar`,
    },
    ios: { state: `ok`, symbol: `ChatRow`, file: `apps/ios/ExpUI/Sources/ChatList.swift`, note: `the select badge on the disc + the bulk bar` },
    android: {
      state: `ok`,
      symbol: `ChatRow`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/ChatComponents.kt`,
      note: `the select badge on the disc + the bulk bar`,
    },
  },
  island: () => <ChatSelectionSpecimen />,
}
