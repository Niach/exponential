// #EDE-65: the Composer's `menuKeys` in the DOM, shared by the React and
// Solid Composer (fixtures/interactions-round1.json `composerKeys`): while
// it is on, Up, Down, Enter, Tab and Escape with no modifier held go to the
// host's suggestion menu as `key` {key} instead of the field. A held
// modifier (Shift+Enter's newline) and an IME composition pass through.

/** The `key` payload's names, the same on every painter. */
export type ComposerMenuKey = `up` | `down` | `enter` | `tab` | `escape`

const MENU_KEYS: Record<string, ComposerMenuKey> = {
  ArrowUp: `up`,
  ArrowDown: `down`,
  Enter: `enter`,
  Tab: `tab`,
  Escape: `escape`,
}

/** What a Composer needs of a DOM keydown (pass the native event). */
export interface ComposerKeyEvent {
  key: string
  ctrlKey: boolean
  altKey: boolean
  shiftKey: boolean
  metaKey: boolean
  isComposing?: boolean
  keyCode?: number
}

/** The menu key a keydown is, or undefined when the field keeps it. */
export function composerMenuKey(e: ComposerKeyEvent): ComposerMenuKey | undefined {
  // keyCode 229 = Safari's keydown that ends a composition.
  if (e.isComposing || e.keyCode === 229) return undefined
  if (e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) return undefined
  return MENU_KEYS[e.key]
}
