import { TRPCClientError } from "@trpc/client"

// EXP-1259 (start surfaces): every remote start carries a client `startId`
// (feedback-states.json `starting.startIdKey`). A PRESS is a new intent and
// mints one; that intent's Retry (a failure toast, a timeout toast, a relay
// timeout) re-sends the SAME id, so the server (`forwardStartOnce`, 30 min)
// and the device drop the repeat instead of starting twice. The composer
// binds its id to the draft (`draftStartKey`); every other surface
// goes through `useRemoteStart` or `useResumeStart`, which apply this.

/** EXP-1259 (feedback-states.json `starting.startIdKey`): a fresh startId,
 *  ONE per press-intent; that intent's Retry re-sends the same one. */
export function newStartId(): string {
  return crypto.randomUUID()
}

/** EXP-1259: what makes a re-send "the same intent" for `useRemoteStart`'s
 *  Retry handoff: the target device and the subject (sorted issue ids, or the
 *  action). A Retry that re-submits anything else mints a fresh id. */
export function startIntentKey(
  deviceId: string,
  subject: { issueIds: readonly string[] } | { actionId: string }
): string {
  return JSON.stringify(
    `issueIds` in subject
      ? [deviceId, `issues`, [...subject.issueIds].sort()]
      : [deviceId, `action`, subject.actionId]
  )
}

/** Recheck R2 (feedback-states.json `starting.fingerprint.fields`): one
 *  draft start as sent. Two drafts are the SAME start (one startId) when
 *  every field is equal: lists compare as sets, absent and null are equal,
 *  text compares verbatim. */
export interface DraftStartFields {
  deviceId: string
  text: string
  issueIds?: readonly string[] | null
  actionId?: string | null
  actionInputs?: Readonly<Record<string, string | null | undefined>> | null
  /** The chat's repo (null for any other subject). */
  repositoryId?: string | null
  /** The local pick ids of the draft's attachments. */
  attachments?: readonly string[] | null
  agent?: string | null
  model?: string | null
  effort?: string | null
  planMode?: boolean | null
  account?: string | null
  mcpServerIds?: readonly string[] | null
  computerUse?: boolean | null
  ultracode?: boolean | null
  subagentModel?: string | null
}

const asSet = (values: readonly string[] | null | undefined): string[] =>
  [...new Set(values ?? [])].sort()

/** Recheck R2: the canonical key of a draft start; equal keys = the same
 *  start (`sameDraftStart`). */
export function draftStartKey(draft: DraftStartFields): string {
  const inputs = Object.entries(draft.actionInputs ?? {})
    .filter((entry): entry is [string, string] => entry[1] != null)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
  return JSON.stringify([
    draft.deviceId,
    draft.text,
    asSet(draft.issueIds),
    draft.actionId ?? null,
    inputs,
    draft.repositoryId ?? null,
    asSet(draft.attachments),
    draft.agent ?? null,
    draft.model ?? null,
    draft.effort ?? null,
    draft.planMode ?? null,
    draft.account ?? null,
    asSet(draft.mcpServerIds),
    draft.computerUse ?? null,
    draft.ultracode ?? null,
    draft.subagentModel ?? null,
  ])
}

/** Recheck R2: `starting.fingerprint.cases`' `same`. */
export function sameDraftStart(a: DraftStartFields, b: DraftStartFields): boolean {
  return draftStartKey(a) === draftStartKey(b)
}

/** Recheck R3 (feedback-states.json `starting.maybeDelivered`): whether a
 *  FAILED start request may still have reached the device. Unknown fate =
 *  a call that got no tRPC answer (a network or transport error, a client
 *  timeout, a proxy page) or the server's relay timeout (504
 *  `GATEWAY_TIMEOUT`): the caller toasts with Retry AND follows the start
 *  quietly for `starting.watchMs`. Any other answer with an error (a 4xx, a
 *  5xx with another error body) is definite: toast + Retry, no follower. */
export function startMaybeDelivered(error: unknown): boolean {
  if (!(error instanceof TRPCClientError)) return true
  const data = error.data as { code?: unknown; httpStatus?: unknown } | undefined
  if (data === undefined || data === null) return true
  return data.code === `GATEWAY_TIMEOUT`
}
