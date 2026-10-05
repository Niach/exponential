import { useCallback, useEffect, useState } from "react"
import {
  BUILTIN_PRIORITY_COLOR_CLASS,
  BUILTIN_STATUS_COLOR_CLASS,
  Button,
  ICON_COMPONENTS,
  IconTooltip,
  Input,
  Pill,
  StatusGlyph,
  Textarea,
  TooltipProvider,
  conceptIcon,
  type PickerItem,
} from "@exp/ui"
import type { IssuePriority, IssueStatus } from "@exp/db-schema/domain"
import { useMcpActions } from "./actions"
import {
  PRIORITY_LABEL,
  STATUS_LABEL,
  normalizePriority,
  normalizeStatus,
  priorityIcon,
  prConcept,
  statusIcon,
  type IssueComment,
  type IssueDetail,
} from "./model"
import { Markdown } from "./markdown"
import { IssueComments, type IssueMember } from "./issue-comments"
import { appOrigin } from "./issue-detail-logic"
import { IssueMenuPicker } from "./issue-menu-picker"
import { IssueStartCoding } from "./issue-start-coding"

const BackIcon = conceptIcon(`ui-back`)
const ExternalIcon = conceptIcon(`ui-external-link`)
const EditIcon = conceptIcon(`ui-edit`)

// The builtin anchors a person picks here. `duplicate` is never offered (the
// app's picker opens the canonical-issue picker for it instead).
const PICKABLE_STATUSES = [
  `backlog`,
  `in_progress`,
  `in_review`,
  `done`,
  `cancelled`,
] as const satisfies readonly IssueStatus[]

const PRIORITIES = [
  `urgent`,
  `high`,
  `medium`,
  `low`,
  `none`,
] as const satisfies readonly IssuePriority[]

const STATUS_ITEMS: PickerItem[] = PICKABLE_STATUSES.map((status) => ({
  value: status,
  label: STATUS_LABEL[status],
  icon: ICON_COMPONENTS[statusIcon(status)],
  color: BUILTIN_STATUS_COLOR_CLASS[status],
}))

const PRIORITY_ITEMS: PickerItem[] = PRIORITIES.map((priority) => ({
  value: priority,
  label: PRIORITY_LABEL[priority],
  icon: ICON_COMPONENTS[priorityIcon(priority)],
  color: BUILTIN_PRIORITY_COLOR_CLASS[priority],
}))

/** `exponential_issues_get` carries the board; the shared type does not. */
type IssueWithBoard = IssueDetail & { boardId?: string }

// EXP-1183 — `exponential_issues_get` as a WORKING issue face, the app's
// issue detail in miniature: an inline-edited title, the status / priority
// pickers, an editable description, "Start coding" on an online device and
// the comment thread with its composer. Every write goes through the host's
// tools/call (`useMcpActions`) and is followed by a fresh issues_get; a
// picked status or priority shows at once and reverts on a refusal.
export function IssueDetailView({
  issue: initial,
  onBack,
  onOpenLink,
}: {
  issue: IssueDetail
  onBack?: () => void
  onOpenLink?: (url: string) => void
}) {
  const { call, openLink } = useMcpActions()
  const open = onOpenLink ?? openLink
  const [issue, setIssue] = useState<IssueWithBoard>(initial)
  const [error, setError] = useState<string | null>(null)
  // A menu's portal sits outside the frame's measured root, so the frame is
  // grown under the face while one is open (the host sizes to content).
  const [menus, setMenus] = useState(0)
  const onMenu = useCallback(
    (isOpen: boolean) => setMenus((count) => Math.max(0, count + (isOpen ? 1 : -1))),
    []
  )
  useEffect(() => setIssue(initial), [initial])

  const issueId = initial.id
  const refresh = useCallback(async () => {
    const result = await call<IssueWithBoard>(`exponential_issues_get`, { id: issueId })
    if (result.kind === `ok`) setIssue(result.data)
  }, [call, issueId])

  const team = useIssueTeam((initial as IssueWithBoard).boardId)

  /** A write with an optimistic patch: shown at once, reverted on refusal. */
  const mutate = async (
    tool: string,
    args: Record<string, unknown>,
    patch: Partial<IssueWithBoard>
  ): Promise<boolean> => {
    const before = issue
    setIssue({ ...issue, ...patch })
    setError(null)
    const result = await call(tool, { id: issue.id, ...args })
    if (result.kind === `error`) {
      setIssue(before)
      setError(result.message)
      return false
    }
    void refresh()
    return true
  }

  const status = normalizeStatus(issue.status)
  const priority = normalizePriority(issue.priority)
  const pr = prConcept(issue.prState)
  const PrIcon = pr ? conceptIcon(pr) : null
  const origin = appOrigin(issue.url)

  return (
    <TooltipProvider>
      <article className="flex flex-col gap-3 px-4 py-3">
        <header className="flex items-center gap-1.5">
          {onBack && (
            <IconTooltip label="Back">
              <Button variant="ghost" size="icon-xs" aria-label="Back to the list" onClick={onBack}>
                <BackIcon />
              </Button>
            </IconTooltip>
          )}
          <span className="font-mono text-xs text-muted-foreground">{issue.identifier}</span>
          {issue.url && (
            <IconTooltip label="Open in Exponential">
              <Button
                variant="ghost"
                size="icon-xs"
                className="text-muted-foreground"
                aria-label="Open in Exponential"
                onClick={() => open(issue.url as string)}
              >
                <ExternalIcon />
              </Button>
            </IconTooltip>
          )}
          <div className="ml-auto">
            <IssueStartCoding
              issueId={issue.id}
              teamId={team.teamId}
              teamResolved={team.resolved}
              onStarted={() => void refresh()}
              onOpenChange={onMenu}
            />
          </div>
        </header>

        <IssueTitle
          title={issue.title}
          onSave={(title) => mutate(`exponential_issues_update`, { title }, { title })}
        />

        <div className="flex flex-wrap items-center gap-1.5">
          <IssueMenuPicker
            label="Status"
            items={STATUS_ITEMS}
            value={status}
            onOpenChange={onMenu}
            onChange={(next) =>
              void mutate(
                `exponential_issues_update_status`,
                { status: next },
                { status: next as IssueStatus }
              )
            }
            trigger={
              <Pill
                mode="action"
                aria-label={`Status: ${STATUS_LABEL[status]}`}
                leading={
                  <StatusGlyph
                    icon={statusIcon(status)}
                    colorClass={BUILTIN_STATUS_COLOR_CLASS[status]}
                  />
                }
              >
                {STATUS_LABEL[status]}
              </Pill>
            }
          />
          <IssueMenuPicker
            label="Priority"
            items={PRIORITY_ITEMS}
            value={priority}
            onOpenChange={onMenu}
            onChange={(next) =>
              void mutate(
                `exponential_issues_update`,
                { priority: next },
                { priority: next as IssuePriority }
              )
            }
            trigger={
              <Pill
                mode="action"
                aria-label={`Priority: ${PRIORITY_LABEL[priority]}`}
                leading={
                  <StatusGlyph
                    icon={priorityIcon(priority)}
                    colorClass={BUILTIN_PRIORITY_COLOR_CLASS[priority]}
                  />
                }
              >
                {PRIORITY_LABEL[priority]}
              </Pill>
            }
          />
          {PrIcon && issue.prUrl && (
            <Pill
              mode="action"
              aria-label="Open the pull request"
              leading={<PrIcon />}
              onClick={() => open(issue.prUrl as string)}
            >
              {issue.prNumber != null ? `#${issue.prNumber}` : `Pull request`}
            </Pill>
          )}
        </div>
        {error && (
          <span className="text-xs text-destructive" role="alert">
            {error}
          </span>
        )}

        <IssueDescription
          description={issue.description ?? ``}
          origin={origin}
          onOpenLink={open}
          onSave={(text) => {
            const description = text.trim() ? text : null
            return mutate(`exponential_issues_update`, { description }, { description })
          }}
        />

        <IssueComments
          issueId={issue.id}
          comments={issue.recentComments ?? []}
          members={team.members}
          origin={origin}
          onOpenLink={open}
          onPosted={(comment: IssueComment | null) => {
            if (comment) {
              setIssue((current) => ({
                ...current,
                recentComments: [comment, ...(current.recentComments ?? [])],
              }))
            }
            void refresh()
          }}
        />
        {menus > 0 && <div aria-hidden className="h-64 shrink-0" />}
      </article>
    </TooltipProvider>
  )
}

/** The issue's team (via its board) and its members by user id — once per
 *  view; a failed lookup leaves names unresolved and devices unshared. */
function useIssueTeam(boardId: string | undefined) {
  const { call } = useMcpActions()
  const [state, setState] = useState<{
    resolved: boolean
    teamId: string | null
    members: ReadonlyMap<string, IssueMember>
  }>({ resolved: false, teamId: null, members: new Map() })
  useEffect(() => {
    let live = true
    void (async () => {
      let teamId: string | null = null
      if (boardId) {
        const board = await call<{ teamId?: string }>(`exponential_boards_get`, { id: boardId })
        if (board.kind === `ok`) teamId = board.data.teamId ?? null
      }
      if (live) setState((current) => ({ ...current, resolved: true, teamId }))
      if (!teamId) return
      const result = await call<IssueMember[]>(`exponential_members_list`, {
        teamId,
        limit: 200,
      })
      if (!live || result.kind !== `ok` || !Array.isArray(result.data)) return
      const members = new Map(result.data.map((member) => [member.id, member]))
      setState((current) => ({ ...current, members }))
    })()
    return () => {
      live = false
    }
  }, [boardId, call])
  return state
}

function IssueTitle({
  title,
  onSave,
}: {
  title: string
  onSave: (title: string) => Promise<boolean>
}) {
  const [draft, setDraft] = useState<string | null>(null)
  const commit = () => {
    if (draft === null) return
    const next = draft.trim()
    setDraft(null)
    if (next && next !== title) void onSave(next)
  }
  if (draft !== null) {
    return (
      <Input
        autoFocus
        value={draft}
        aria-label="Issue title"
        maxLength={500}
        className="h-auto py-1 text-lg font-semibold md:text-lg"
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return
          if (event.key === `Enter`) {
            event.preventDefault()
            commit()
          } else if (event.key === `Escape`) {
            event.preventDefault()
            setDraft(null)
          }
        }}
      />
    )
  }
  return (
    <h1
      role="button"
      tabIndex={0}
      aria-label={`Edit title: ${title}`}
      title="Click to edit"
      className="-mx-1 cursor-text rounded-md px-1 text-lg font-semibold leading-tight outline-none hover:bg-glass-row focus-visible:ring-[3px] focus-visible:ring-ring/50"
      onClick={() => setDraft(title)}
      onKeyDown={(event) => {
        if (event.key === `Enter` || event.key === ` `) {
          event.preventDefault()
          setDraft(title)
        }
      }}
    >
      {title}
    </h1>
  )
}

function IssueDescription({
  description,
  origin,
  onOpenLink,
  onSave,
}: {
  description: string
  origin?: string
  onOpenLink: (url: string) => void
  onSave: (text: string) => Promise<boolean>
}) {
  const [draft, setDraft] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  const save = async () => {
    if (draft === null) return
    setSaving(true)
    const ok = await onSave(draft)
    setSaving(false)
    if (ok) setDraft(null)
  }

  if (draft !== null) {
    return (
      <div className="flex flex-col gap-2">
        <Textarea
          autoFocus
          value={draft}
          aria-label="Description (Markdown)"
          placeholder="Add a description…"
          disabled={saving}
          className="max-h-[28rem] min-h-32 font-mono text-xs md:text-xs"
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === `Escape`) {
              event.preventDefault()
              setDraft(null)
            } else if (event.key === `Enter` && (event.metaKey || event.ctrlKey)) {
              event.preventDefault()
              void save()
            }
          }}
        />
        <div className="flex justify-end gap-1.5">
          <Button size="xs" variant="ghost" disabled={saving} onClick={() => setDraft(null)}>
            Cancel
          </Button>
          <Button size="xs" disabled={saving || draft === description} onClick={() => void save()}>
            {saving ? `Saving…` : `Save`}
          </Button>
        </div>
      </div>
    )
  }

  return (
    <div className="group relative flex flex-col gap-1">
      {description.trim() ? (
        <Markdown source={description} origin={origin} onOpenLink={onOpenLink} />
      ) : (
        <p className="text-sm text-muted-foreground">No description.</p>
      )}
      <Button
        variant="text"
        size="inline"
        className="self-start"
        aria-label="Edit the description"
        onClick={() => setDraft(description)}
      >
        <EditIcon />
        Edit
      </Button>
    </div>
  )
}
