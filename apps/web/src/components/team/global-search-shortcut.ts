import { findInChatKey } from '@exp/domain-contract/find-in-chat'

/** One window keydown as the app's global search shortcut reads it. */
export type GlobalSearchKeyEvent = Pick<KeyboardEvent, `key` | `metaKey` | `ctrlKey` | `shiftKey` | `defaultPrevented`>

/**
 * #EXP-2025 (find-in-chat.json `keyCases`): Ctrl/Cmd+K opens the global
 * search everywhere, a chat too; Ctrl/Cmd+F does outside a chat (an open
 * chat's find in chat handles it first and prevents the default).
 */
export function isGlobalSearchKey(event: GlobalSearchKeyEvent): boolean {
  if (event.defaultPrevented) return false
  return findInChatKey({ key: event.key, mod: event.metaKey || event.ctrlKey, shift: event.shiftKey, inChat: false, inField: false }) === `global`
}
