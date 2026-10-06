import fixture from "@exp/domain-contract/fixtures/issue-draft.json"
import {
  isFallbackStatusOption,
  type StatusRowOption,
} from "@/lib/team-statuses"
import {
  capturedOrigin,
  formatOrigin,
  parseOrigin,
  screenFromPath,
} from "@/lib/detail-origin"

// EXP-1170: the New issue PAGE, locked ×4 against the ONE contract fixture
// (desktop domain::issue_draft, iOS ExpCore IssueDraftPage, Android
// domain/IssueDraftPage). There is no create dialog any more: every "New
// issue" opener mints a draft id and navigates to the issue detail in DRAFT
// mode, which autosaves to `issue_drafts` until Create files it.

/** The page's copy: header slot, placeholders, the two actions, the list label. */
export const ISSUE_DRAFT_COPY = fixture.copy

/** Quiet time after the last title/description edit before the draft is written. */
export const ISSUE_DRAFT_AUTOSAVE_MS = fixture.autosave.debounceMs

/** The draft page's search params: the board it files onto, the group
 *  status a "+" seeded, and the `?from=` origin (`lib/detail-origin.ts`). */
export interface IssueDraftSearch {
  board?: string
  status?: string
  from?: string
}

export function parseIssueDraftSearch(
  search: Record<string, unknown>
): IssueDraftSearch {
  const pick = (value: unknown) =>
    typeof value === `string` && value ? value : undefined
  return {
    board: pick(search.board),
    status: pick(search.status),
    from: pick(search.from),
  }
}

/**
 * EXP-1170: the ONE "New issue" navigation every opener runs. The draft id is
 * minted HERE, at tap time, so the page's every write is the same idempotent
 * upsert of that id. A constructed fallback status (`builtin:<key>`, shape not
 * synced yet) never travels: the page then lands on the team's Backlog, which
 * is what the fallback means anyway.
 */
export function newDraftNavigation({
  teamSlug,
  boardId,
  status,
  from,
  draftId = crypto.randomUUID(),
}: {
  teamSlug: string
  boardId?: string
  status?: StatusRowOption
  from?: string
  draftId?: string
}) {
  const search: IssueDraftSearch = {}
  if (boardId) search.board = boardId
  if (status && !isFallbackStatusOption(status)) search.status = status.id
  if (from) search.from = from
  return {
    to: `/t/$teamSlug/drafts/$draftId` as const,
    params: { teamSlug, draftId },
    search,
  }
}

/** Reopening an existing draft (the Drafts lists): same page, its own id. */
export function openDraftNavigation({
  teamSlug,
  draftId,
  boardId,
  from,
}: {
  teamSlug: string
  draftId: string
  boardId?: string
  from?: string
}) {
  return newDraftNavigation({ teamSlug, draftId, boardId, from })
}

/**
 * The `?from=` a New issue opener hands the draft page: the list in force
 * where the tap happened (a board, the inbox, the list an open issue carries),
 * so md+ keeps that list beside the draft and Back returns to it. Context-free
 * screens hand on nothing.
 */
export function draftOriginFrom(
  pathname: string,
  from: string | null | undefined
): string | undefined {
  return formatOrigin(capturedOrigin(screenFromPath(pathname), parseOrigin(from)))
}

/**
 * EXP-1212: how the page leaves. `discard` = its own close button, `leave` =
 * any other in-app navigation (Back, a nav entry, another screen). The page's
 * own exits after a successful Create or a confirmed Discard never ask.
 */
export type DraftExitTrigger = `discard` | `leave`

/** Which prompt an exit raises: none (go at once), the destructive discard
 *  confirm, or the three-choice leave dialog. */
export type DraftExitPrompt = `none` | `discardConfirm` | `leave`

/** A draft WITH content (title, description or attachment) never goes
 *  silently; an empty one goes at once, as before. */
export function draftExitPrompt(
  trigger: DraftExitTrigger,
  hasContent: boolean
): DraftExitPrompt {
  if (!hasContent) return `none`
  return trigger === `discard` ? `discardConfirm` : `leave`
}

/** The page's Create (and the leave dialog's): a title, nothing filing yet,
 *  no eager upload still in flight. */
export function canCreateDraft({
  title,
  creating,
  uploading,
}: {
  title: string
  creating: boolean
  uploading: boolean
}): boolean {
  return title.trim().length > 0 && !creating && !uploading
}
