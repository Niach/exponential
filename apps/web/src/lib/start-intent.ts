// EXP-1259 (start surfaces): every remote start carries a client `startId`
// (feedback-states.json `starting.startIdKey`). A PRESS is a new intent and
// mints one; that intent's Retry (a failure toast, a timeout toast, a relay
// timeout) re-sends the SAME id, so the server (`forwardStartOnce`, 30 min)
// and the device drop the repeat instead of starting twice. The composer
// binds its id to the draft (`draftStartFingerprint`); every other surface
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
