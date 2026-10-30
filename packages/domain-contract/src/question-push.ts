// #VAPP-45 (fixtures/question-push.json, its `$comment` is the spec): the
// push of a question or plan card carries the question TEXT end to end,
// sealed to the tree. The server never opens it; a phone opens `sealedText`
// with the tree key it holds and shows THE chat ask preview as the body.
// Mirrors: iOS ExpPush `QuestionPush.swift` (the NSE), Android
// `push/QuestionPush.kt` (FcmService).

import fixture from "../fixtures/question-push.json" with { type: "json" }
import { askPreview } from "./chats"
import { openBody, type TreeKeys } from "./sealed-content"
import type { TreeMessage } from "./session-tree"

export const QUESTION_PUSH_COPY = fixture.copy
/** The shown body's cap in code points (longer = cut, ending in `…`). */
export const QUESTION_PUSH_BODY_MAX = fixture.bodyMax
/** The push `data` keys this feature adds. */
export const QUESTION_PUSH_DATA_KEYS = fixture.dataKeys

/** Who a question/plan card a session posts is pushed to: a ROOT's owner
 *  alone; a child's ask routes to its parent agent (bus), never a push. */
export function questionPushRecipients(session: { parentId: string | null; userId: string }): string[] {
  return session.parentId === null ? [session.userId] : []
}

/** The push title: an Assistant conversation's decision copy, else the
 *  session's title + `copy.title`. */
export function questionPushTitle(args: { assistant: boolean; sessionTitle: string }): string {
  return args.assistant ? fixture.copy.assistantTitle : fixture.copy.title.replace(`{title}`, args.sessionTitle)
}

/** `text` cut to `bodyMax` code points (`…` last). */
export function clipQuestionPushBody(text: string): string {
  const chars = [...text]
  return chars.length > fixture.bodyMax ? `${chars.slice(0, fixture.bodyMax - 1).join(``)}…` : text
}

/** The shown body of an ask text (the bus's `askText`: opened, or the
 *  server's own for a plaintext card): THE chat ask preview, clipped. */
export function questionPushText(askText: string): string {
  const message = { card: { kind: `ask` }, body: askText, authorSessionId: null } as unknown as Pick<
    TreeMessage,
    `card` | `body` | `authorSessionId`
  >
  return clipQuestionPushBody(askPreview(message).text)
}

/** A phone's body for a delivered push: `data.sealedText` opened with the
 *  tree keys of `data.rootId` (`keysFor`), else `fallback` (the push's own
 *  body, the generic copy for a sealed card). */
export function questionPushBody(
  data: Readonly<Record<string, string | undefined>>,
  keysFor: (rootId: string) => TreeKeys | null | undefined,
  fallback: string
): string {
  const sealed = data[fixture.dataKeys.sealedText]
  const rootId = data[fixture.dataKeys.rootId]
  if (!sealed || !rootId) return fallback
  const keys = keysFor(rootId)
  if (!keys) return fallback
  const opened = openBody(sealed, keys)
  if (opened.status !== `opened`) return fallback
  const text = questionPushText(opened.text)
  return text.trim() ? text : fallback
}
