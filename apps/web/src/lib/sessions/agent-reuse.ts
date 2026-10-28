// S-3 (owner 2026-10-28): when an agent's message may reuse a thread root
// (`messageSession` → `agentFreshThread`). Pure: the thresholds are
// contract `sessionsSend.reuse*`.

import { contract } from "@exp/domain-contract"
import type { Session } from "@exp/db-schema/schema"

const HOUR = 60 * 60 * 1000

/**
 * S-3: may an AGENT's message reuse this thread root? Only while it is not
 * ended or ended less than `sessionsSend.reuseMaxEndedHours` ago, its last
 * turn (else activity) is under `reuseMaxIdleHours` old and the tree holds
 * fewer than `reuseMaxChildren` sub-sessions. Null = reuse; else the reason
 * (the fresh thread's `warning`).
 */
export function agentReuseRefusal(
  root: Pick<Session, `status` | `endedAt` | `lastTurnAt` | `lastActivityAt` | `updatedAt`>,
  children: number,
  now: Date
): string | null {
  const rules = contract.sessionsSend
  const age = (at: Date | null) => (at ? now.getTime() - at.getTime() : 0)
  if (root.status === `ended` && age(root.endedAt ?? root.updatedAt) >= rules.reuseMaxEndedHours * HOUR) {
    return `it ended more than ${rules.reuseMaxEndedHours} h ago`
  }
  if (age(root.lastTurnAt ?? root.lastActivityAt ?? root.updatedAt) >= rules.reuseMaxIdleHours * HOUR) {
    return `its last turn is more than ${rules.reuseMaxIdleHours} h old`
  }
  if (children >= rules.reuseMaxChildren) return `it already has ${children} sub-sessions`
  return null
}

