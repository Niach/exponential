// #EXP-1629: the KeyRecorder field in the DOM, shared by the React and Solid
// Box (fixtures/interactions-round1.json `keyRecorder`): a DOM key or mouse
// event → the `record` payload every painter sends, with the CANONICAL key
// name (the physical key, never the shifted character). The grammar that
// turns those into a binding stays with the host.

/** The payload of a field's `record` event (and so the context of `key`). */
export interface KeyRecorderPayload {
  type: `down` | `up` | `modifiers` | `mouse`
  key?: string
  button?: number
  ctrl: boolean
  alt: boolean
  shift: boolean
  meta: boolean
}

/** The held flags of a DOM event. */
interface ModifierFlags {
  ctrlKey: boolean
  altKey: boolean
  shiftKey: boolean
  metaKey: boolean
}

const NAMED: Record<string, string> = {
  Space: `space`,
  Enter: `enter`,
  NumpadEnter: `enter`,
  Tab: `tab`,
  Escape: `escape`,
  Backspace: `backspace`,
  Delete: `delete`,
  Insert: `insert`,
  Home: `home`,
  End: `end`,
  PageUp: `pageup`,
  PageDown: `pagedown`,
  ArrowUp: `up`,
  ArrowDown: `down`,
  ArrowLeft: `left`,
  ArrowRight: `right`,
  Minus: `-`,
  Equal: `=`,
  BracketLeft: `[`,
  BracketRight: `]`,
  Backslash: `\\`,
  Semicolon: `;`,
  Quote: `'`,
  Comma: `,`,
  Period: `.`,
  Slash: `/`,
  Backquote: `\``,
}

const MODIFIER_CODES = new Set([`ControlLeft`, `ControlRight`, `ShiftLeft`, `ShiftRight`, `AltLeft`, `AltRight`, `MetaLeft`, `MetaRight`, `OSLeft`, `OSRight`])

/** The canonical name of a DOM `KeyboardEvent.code`; undefined = not sent. */
export function keyRecorderKey(code: string): string | undefined {
  const letter = /^Key([A-Z])$/.exec(code)
  if (letter) return letter[1]!.toLowerCase()
  const digit = /^Digit([0-9])$/.exec(code)
  if (digit) return digit[1]
  const fn = /^F([1-9]|1[0-9]|2[0-4])$/.exec(code)
  if (fn) return `f${fn[1]}`
  return NAMED[code]
}

/** The DOM `KeyboardEvent.code` of a canonical name (the replays' inverse). */
export function keyRecorderCode(key: string): string | undefined {
  if (/^[a-z]$/.test(key)) return `Key${key.toUpperCase()}`
  if (/^[0-9]$/.test(key)) return `Digit${key}`
  if (/^f([1-9]|1[0-9]|2[0-4])$/.test(key)) return key.toUpperCase()
  return Object.entries(NAMED).find(([, name]) => name === key)?.[0]
}

const flags = (e: ModifierFlags) => ({ ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: e.metaKey })

/** A keydown / keyup while recording: a modifier key alone = `modifiers`
 *  (the flags after it), another key = `down` / `up` with its name; a key
 *  repeat or a key outside the canonical list = undefined (send nothing,
 *  still swallow it). */
export function keyRecorderKeyPayload(e: ModifierFlags & { type: string; code: string; repeat?: boolean }): KeyRecorderPayload | undefined {
  if (e.repeat) return undefined
  if (MODIFIER_CODES.has(e.code)) return { type: `modifiers`, ...flags(e) }
  const key = keyRecorderKey(e.code)
  if (!key) return undefined
  return { type: e.type === `keyup` ? `up` : `down`, key, ...flags(e) }
}

/** A mouse button pressed over the field while recording: DOM 1 middle,
 *  3 back, 4 forward = buttons 3, 4, 5; the left and right buttons send
 *  nothing. */
export function keyRecorderMousePayload(e: ModifierFlags & { button: number }): KeyRecorderPayload | undefined {
  const button = ({ 1: 3, 3: 4, 4: 5 } as Record<number, number>)[e.button]
  return button ? { type: `mouse`, button, ...flags(e) } : undefined
}

/** Whether a node's recipe is a KeyRecorder's field (`recipe.macro` / `part`). */
export function isKeyRecorderField(recipe: { macro?: string; part?: string } | undefined): boolean {
  return recipe?.macro === `KeyRecorder` && recipe.part === `field`
}
