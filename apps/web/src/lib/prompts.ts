import fixture from "@exp/domain-contract/fixtures/prompts.json"
import type { PromptAction, PromptActionRole } from "@exp/ui"
import { PLAN_LIMITS } from "@/lib/plan-limits"

// EXP-1215: the wording of every confirm/choice prompt, locked ×3 (iOS
// ExpUI GlassAlert, Android GlassAlert) against the ONE contract fixture
// `prompts.json`. A prompt site renders an entry by id through the helpers
// below (title, body, the actions in display order with their roles and the
// initial focus) and attaches its handlers by action id with
// `promptActions`; it never restates a string. Prompts the fixture does not
// list because only the web shows them (settings, billing, admin) follow the
// same rules and live in `WEB_PROMPTS` at the bottom.

export const PROMPT_FIXTURE = fixture.prompts

export type PromptId = keyof typeof fixture.prompts

/** One answer of a prompt: the fixture's id, label and role, plus focus. */
export interface PromptCopyAction {
  id: string
  label: string
  role: PromptActionRole
  autoFocus?: boolean
}

/** The ready text of one prompt: spread `title`/`body` onto `Prompt`, hand
 *  `actions` to `promptActions` with the site's handlers. */
export interface PromptCopy {
  title: string
  body?: string
  actions: readonly PromptCopyAction[]
}

type FixtureAction = { id: string; label: string; role: string }

/** Fills `{name}` placeholders; an unknown placeholder stays verbatim. */
export function fillPromptTemplate(
  template: string,
  params: Record<string, string | number> = {}
): string {
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match
  )
}

function actionsOf(
  actions: readonly FixtureAction[],
  focus: string
): PromptCopyAction[] {
  return actions.map((action) => ({
    id: action.id,
    label: action.label,
    role: action.role as PromptActionRole,
    ...(action.id === focus ? { autoFocus: true } : {}),
  }))
}

function copyOf(
  entry: { actions: readonly FixtureAction[]; focus: string },
  title: string,
  body: string | undefined,
  params: Record<string, string | number> = {}
): PromptCopy {
  return {
    title: fillPromptTemplate(title, params),
    ...(body === undefined ? {} : { body: fillPromptTemplate(body, params) }),
    actions: actionsOf(entry.actions, entry.focus),
  }
}

const P = fixture.prompts

/** What a site attaches to one answer, keyed by the action's id. */
export type PromptHandler = Omit<PromptAction, `label` | `role` | `autoFocus`>

/**
 * The `Prompt` actions of a copy, in display order, with each answer's
 * handler (`onSelect`, `busy`, `disabled`, `testId`, `leading`) attached by
 * its id. The `cancel` answer needs none: no `onSelect` = dismiss.
 */
export function promptActions(
  copy: PromptCopy,
  handlers: Record<string, PromptHandler> = {}
): PromptAction[] {
  return copy.actions.map(({ id, ...action }) => ({
    ...action,
    ...handlers[id],
  }))
}

// ── The contract prompts (fixture `prompts`) ─────────────────────────────────

export const deleteIssuePrompt = (identifier: string) =>
  copyOf(P[`delete-issue`], P[`delete-issue`].title, P[`delete-issue`].body, {
    identifier,
  })

export const deleteIssuesPrompt = (count: number) => {
  const entry = P[`delete-issues`]
  return count === 1
    ? copyOf(entry, entry.titleOne, entry.bodyOne)
    : copyOf(entry, entry.titleMany, entry.bodyMany, { count })
}

export const deleteFilePrompt = (filename: string) =>
  copyOf(P[`delete-file`], P[`delete-file`].title, P[`delete-file`].body, {
    filename,
  })

export const moveIssuePrompt = (identifier: string, board: string) =>
  copyOf(P[`move-issue`], P[`move-issue`].title, P[`move-issue`].body, {
    identifier,
    board,
  })

/** `count` = the issues the PR links (a batch PR: 2 or more). */
export const mergeIssuePrPrompt = ({
  number,
  count,
}: {
  number?: number | null
  count: number
}) => {
  const entry = P[`merge-issue-pr`]
  return copyOf(
    entry,
    number == null ? entry.titleNoNumber : entry.title,
    count > 1 ? entry.bodyMany : entry.bodyOne,
    { ...(number == null ? {} : { number }), count }
  )
}

export const mergeRunPrPrompt = (number?: number | null) => {
  const entry = P[`merge-run-pr`]
  return copyOf(
    entry,
    number == null ? entry.titleNoNumber : entry.title,
    entry.body,
    number == null ? {} : { number }
  )
}

export const stopRunPrompt = () =>
  copyOf(P[`stop-run`], P[`stop-run`].title, undefined)

export const resumeRunPrompt = (device?: string | null) => {
  const entry = P[`resume-run`]
  return device
    ? copyOf(entry, entry.title, entry.body, { device })
    : copyOf(entry, entry.titleNoDevice, entry.body)
}

export const deleteTriggerPrompt = () =>
  copyOf(P[`delete-trigger`], P[`delete-trigger`].title, P[`delete-trigger`].body)

export const deleteActionPrompt = (name: string) =>
  copyOf(P[`delete-action`], P[`delete-action`].title, P[`delete-action`].body, {
    name,
  })

export const removeDevicePrompt = (name: string) =>
  copyOf(P[`remove-device`], P[`remove-device`].title, P[`remove-device`].body, {
    name,
  })

export const deleteTeamPrompt = (name: string) =>
  copyOf(P[`delete-team`], P[`delete-team`].title, P[`delete-team`].body, {
    name,
  })

export const trashBoardPrompt = (name: string) =>
  copyOf(P[`trash-board`], P[`trash-board`].title, P[`trash-board`].body, {
    name,
  })

export const deleteLabelPrompt = (name: string) =>
  copyOf(P[`delete-label`], P[`delete-label`].title, P[`delete-label`].body, {
    name,
  })

export const removeMemberPrompt = (name: string) =>
  copyOf(P[`remove-member`], P[`remove-member`].title, P[`remove-member`].body, {
    name,
  })

export const leaveTeamPrompt = (name: string) =>
  copyOf(P[`leave-team`], P[`leave-team`].title, P[`leave-team`].body, { name })

export const makeOwnerPrompt = (name: string) =>
  copyOf(P[`make-owner`], P[`make-owner`].title, P[`make-owner`].body, { name })

export const makeMemberPrompt = (name: string) =>
  copyOf(P[`make-member`], P[`make-member`].title, P[`make-member`].body, {
    name,
  })

export const removeRepositoryPrompt = (fullName: string) =>
  copyOf(P[`remove-repository`], P[`remove-repository`].title, undefined, {
    fullName,
  })

export const unlinkSignInMethodPrompt = (provider: string) =>
  copyOf(
    P[`unlink-sign-in-method`],
    P[`unlink-sign-in-method`].title,
    P[`unlink-sign-in-method`].body,
    { provider }
  )

export const removePasswordPrompt = () =>
  copyOf(
    P[`remove-password`],
    P[`remove-password`].title,
    P[`remove-password`].body
  )

export const removePasskeyPrompt = (name: string) =>
  copyOf(
    P[`remove-passkey`],
    P[`remove-passkey`].title,
    P[`remove-passkey`].body,
    { name }
  )

/** Web: the consequences list replaces `body` (the entry's `slot`), so the
 *  site renders `body` only where it has no list. */
export const deleteAccountPrompt = (server?: string) => {
  const entry = P[`delete-account`]
  return server
    ? copyOf(entry, entry.titleOnServer, entry.body, { server })
    : copyOf(entry, entry.title, entry.body)
}

export const removeServerPrompt = (server: string) =>
  copyOf(P[`remove-server`], P[`remove-server`].title, P[`remove-server`].body, {
    server,
  })

// ── Web-only prompts (not in the fixture until a second client shows them) ──

const cancelAction = (): PromptCopyAction => ({
  id: `cancel`,
  label: `Cancel`,
  role: `cancel`,
})

/** A plain destructive confirm: Cancel (focus) + the destructive verb. */
const destructiveConfirm = (
  title: string,
  body: string | undefined,
  id: string,
  label: string
): PromptCopy => ({
  title,
  ...(body === undefined ? {} : { body }),
  actions: [
    { ...cancelAction(), autoFocus: true },
    { id, label, role: `destructive` },
  ],
})

/** A safe confirm: Cancel + the primary verb (focus). */
const primaryConfirm = (
  title: string,
  body: string | undefined,
  id: string,
  label: string
): PromptCopy => ({
  title,
  ...(body === undefined ? {} : { body }),
  actions: [cancelAction(), { id, label, role: `primary`, autoFocus: true }],
})

const FREE = PLAN_LIMITS.free

export const WEB_PROMPTS = {
  updateDevice: (name: string) =>
    primaryConfirm(
      `Update "${name}" now?`,
      `Every live run on it ends. Runs with a repository can be resumed.`,
      `update`,
      `Update`
    ),
  sweepImages: (count: number) =>
    destructiveConfirm(
      count === 1
        ? `Delete 1 unreferenced image?`
        : `Delete ${count} unreferenced images?`,
      `An unsaved draft may still use them. Uploads from the last 24 hours are kept.`,
      `delete`,
      `Delete`
    ),
  deleteWidget: (name: string) =>
    destructiveConfirm(
      `Delete the widget "${name}"?`,
      `Sites that use its key stop working immediately. Its issues are kept.`,
      `delete`,
      `Delete`
    ),
  /** `count` = issues on the status; null while unknown, 0 = no body. */
  deleteStatus: (name: string, count: number | null) =>
    destructiveConfirm(
      `Delete "${name}"?`,
      count === null || count === 0
        ? undefined
        : count === 1
          ? `1 issue moves to the status you pick.`
          : `${count} issues move to the status you pick.`,
      `delete`,
      `Delete`
    ),
  /** `date` null = the period end is unknown. */
  cancelSubscription: (plan: string, date: string | null): PromptCopy => ({
    title: `Cancel the subscription?`,
    body: `The team keeps ${plan} until ${date ?? `the end of the paid period`}, then drops to Free: ${FREE.seats} seats, ${FREE.storageMb} MB, ${FREE.widgetConfigs} widget. You can resume before then.`,
    actions: [
      { id: `keep`, label: `Keep subscription`, role: `cancel`, autoFocus: true },
      { id: `cancel-subscription`, label: `Cancel subscription`, role: `destructive` },
    ],
  }),
  removeMcpServer: (name: string) =>
    destructiveConfirm(
      `Remove "${name}"?`,
      `Every member's connection to it is deleted.`,
      `remove`,
      `Remove`
    ),
  revokeApiKey: (name: string, preview: string) =>
    destructiveConfirm(
      `Revoke "${name}"?`,
      `Anything that uses ${preview} stops working immediately.`,
      `revoke`,
      `Revoke`
    ),
  disconnectDeviceKey: (name: string) =>
    destructiveConfirm(
      `Disconnect "${name}"?`,
      `Its runs lose their Exponential tools until it signs in again.`,
      `disconnect`,
      `Disconnect`
    ),
  adminDeleteUser: (email: string) =>
    destructiveConfirm(
      `Delete ${email}?`,
      `Their team memberships, issues and comments are deleted with them.`,
      `delete`,
      `Delete`
    ),
  adminCompTier: (name: string, tier: string) =>
    primaryConfirm(
      `Give "${name}" the ${tier} tier for free?`,
      `A paid subscription of a higher tier still wins.`,
      `comp`,
      `Give`
    ),
  adminClearCompTier: (name: string) =>
    primaryConfirm(
      `Clear the comp tier of "${name}"?`,
      `It falls back to its paid plan, or Free.`,
      `clear`,
      `Clear`
    ),
  archiveBoard: (name: string) =>
    primaryConfirm(
      `Archive "${name}"?`,
      `It and its issues disappear for the whole team. An owner can bring it back in team settings.`,
      `archive`,
      `Archive`
    ),
} as const
