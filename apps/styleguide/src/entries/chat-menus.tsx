import fixture from "@exp/domain-contract/fixtures/chats.json"
import { ChatMenusSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// Thread 58cb32df (fixtures/chats.json MENUS ×4): THE chat menu and THE agent
// menu in both looks plus the delete confirm, `@exp/ui` `ChatMenusSpecimen`
// (the app's `?dev=chat-menus` capture renders the same).

const COPY = fixture.copy

export const entry: StyleguideEntry = {
  id: `chat-menus`,
  section: `special`,
  owner: `EXP-1373`,
  title: `Chat menus`,
  blurb: `The menus a chat row and an agent open, identical ×4 through each platform's ONE menu host (a right-click on pointers, a long-press on touch; phones present the sheet with the chat's title as its header). A chat (the phone list, the Chats page rows, the rail's chat rows and the chat header's \`…\`): S-3: ONE entry, the destructive "${COPY.menu.delete}" (no Stop anywhere), for a viewer who can steer it and may delete it; anyone else gets no menu. A delete asks first ("${COPY.confirmDelete.title}"), then closes the open chat back to the list. The agent menu (the agent chat's \`…\`, a bubble's long-press, the rail's agent row): "${COPY.menu.compact}" and "${COPY.menu.newConversation}" SEND \`${fixture.agentMenu.sends.compact}\` / \`${fixture.agentMenu.sends.newConversation}\` (one path with the typed commands), "${COPY.menu.settings}" for team owners. A failure toasts "${COPY.toasts.deleteFailed}". The composer's stop-turn is the only stop.`,
  status: {
    web: {
      state: `ok`,
      symbol: `chatMenuRows / agentMenuRows`,
      file: `packages/ui/src/threads-list.tsx`,
      note: `apps/web/src/components/thread/chat-menu.tsx: kinds \`thread\` + \`agent\` in the team layout's MenuGestureHost, the header \`…\` menus, the Prompt confirm`,
    },
    desktop: { state: `ok`, symbol: `chat_actions::chat_menu_items`, file: `apps/desktop/crates/ui/src/chat_actions.rs`, note: `PointerContextMenu; twin: styleguide/entries/chat_menus.rs` },
    ios: { state: `ok`, symbol: `ChatMenuSheet`, file: `apps/ios/Exponential/UI/Chats/ChatMenuSheet.swift`, note: `on ExpUI GlassLongPressMenu` },
    android: { state: `ok`, symbol: `ChatMenuSheet / ChatDeleteConfirm`, file: `apps/android/app/src/main/java/com/exponential/app/ui/components/ChatComponents.kt`, note: `combinedClickable long-press` },
  },
  island: () => <ChatMenusSpecimen />,
}
