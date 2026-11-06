// VAPP-260: the Engine's outbox IS O1's (`@exp/domain-contract/app-records`,
// fixture `app-records.json` `outbox`), re-exported here; the Engine adds only
// WHEN it applies (`fixtures/outbox.json` `modeCases`).

export {
  outboxFlush,
  outboxOutcome,
  outboxPendingLabel,
  outboxPrune,
  outboxReflected,
  outboxRequest,
  outboxRows,
  outboxVerb,
  outboxWrites,
  type OutboxCall,
  type OutboxEntry,
  type OutboxOutcome,
  type OutboxResult,
  type OutboxWrite,
} from "@exp/domain-contract/app-records"

/** A host with a sync provider queues every app write; one without writes through. */
export const outboxMode = (hasSync: boolean): `queue` | `direct` => (hasSync ? `queue` : `direct`)
