import chats from "@exp/domain-contract/fixtures/chats.json"
import { ChatHeaderSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1688 (`chats.json` `shell.chatHeader`): THE chat header on a phone,
// `@exp/ui` `ChatHeaderSpecimen`; the app's `?dev=chat-header` capture renders
// the same.

const HEADER = chats.shell.chatHeader

export const entry: StyleguideEntry = {
  id: `chat-header`,
  section: `special`,
  owner: `EXP-1688`,
  title: `Chat header`,
  blurb: `THE chat header ×3 on every chat, an agent's too: one ${HEADER.row}px row, always leading: the ghost back button, the ${HEADER.disc}px lead disc, the ${HEADER.title}px semibold title over one ${HEADER.subtitle}px subtitle line; trailing the Merge pill, an agent chat's telephone, then \`…\`. ${HEADER.button}px ghost buttons: no fill, no hairline.`,
  status: {
    web: {
      state: `ok`,
      symbol: `MobileDetailHeader + PhoneHeaderTitle`,
      file: `packages/ui/src/phone-header-title.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `render`,
      file: `apps/desktop/crates/ui/src/styleguide/entries/chat_header.rs`,
      note: `the IDE has no phone shell; the styleguide twin paints the same header`,
    },
    ios: {
      state: `ok`,
      symbol: `ChatHeader`,
      file: `apps/ios/Exponential/UI/Thread/ThreadStripTabs.swift`,
    },
    android: {
      state: `ok`,
      symbol: `ChatHeader`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/thread/ChatHeader.kt`,
    },
  },
  island: () => <ChatHeaderSpecimen />,
}
