import type { ActionTrigger, ActionTriggerInput } from "@exp/db-schema/domain"

/** A trigger as written: one the action already holds, or a new one the
 * server mints the id for. */
export type TriggerWrite = ActionTriggerInput | ActionTrigger
