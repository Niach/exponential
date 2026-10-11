// EXP-1274: the ×4 empty-state copy (`@exp/domain-contract/fixtures/
// empty-state.json`) — every empty state two or more clients draw, keyed by
// a stable id, with its size, concept icon, title and (page only) the
// teaching sentence + the primary button's label. `components/empty.tsx`
// renders one; the natives read the same file in their tests.
import fixture from "@exp/domain-contract/fixtures/empty-state.json"

export type EmptyStateId = keyof typeof fixture.states

export interface EmptyStateCopy {
  size: `page` | `list`
  icon: string
  title: string
  description?: string
  action?: string
}

export const EMPTY_STATES: Record<EmptyStateId, EmptyStateCopy> =
  fixture.states as Record<EmptyStateId, EmptyStateCopy>

export function emptyStateCopy(id: EmptyStateId): EmptyStateCopy {
  return EMPTY_STATES[id]
}
