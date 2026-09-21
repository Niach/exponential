import { useMemo, type ReactNode } from "react"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import {
  entityChipLabel,
  entityKindNoun,
  entityRefIcon,
} from "@exp/domain-contract/entity-preview"
import type { IconConcept } from "@exp/icons"
import {
  AgentBrandMark,
  BoardGlyph,
  EntityPreviewCard,
  LiveDot,
  Pill,
  StatusGlyph,
  TeamAvatar,
  UserAvatar,
  conceptIcon,
  getActionIcon,
  getDeviceIcon,
  type EntityPreviewRow,
  type LiveDotTone,
} from "@exp/ui"
import type {
  Action,
  Attachment,
  Automation,
  Board,
  CodingSession,
  Comment,
  Device,
  Issue,
  Label,
  Notification,
  SyncedWorkflow,
  Team,
  TeamInvite,
  TeamMember,
  User,
} from "@/db/schema"
import {
  actionCollection,
  deviceCollection,
  issueCollection,
  teamMemberCollection,
  userCollection,
} from "@/lib/collections"
import type { EntityRef } from "@/lib/mcp/preview"
import { useIssueRefs, type ResolvedIssueRef } from "@/components/issue-ref-provider"
import { IssuePreviewCard } from "@/components/issue-preview-card"
import { IssueChip } from "@/components/issue-chip"
import { StatusIcon, statusColorClass } from "@/components/issue-properties/status-dropdown"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { useTeamRepos } from "@/hooks/use-team-repos"
import { useSessionDevice } from "@/hooks/use-session-device"
import { useNow } from "@/hooks/use-now"
import type { StatusRowOption } from "@/lib/team-statuses"
import { relativeTime } from "@/components/comment-rows/format"
import { displayUserName } from "@/lib/user-display"
import { formatAttachmentSize } from "@/lib/attachment-files"
import { parseAutomationTrigger, triggerSummary } from "@/lib/action-triggers"
import { sessionIdentity } from "@/lib/session-identity"
import { sessionDisplayState, sessionStatusLine } from "@/lib/coding-session-display"
import { sessionIsPaused } from "@/lib/session-device"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import { workflowRowSubtitle } from "@/lib/workflow-view"
import { isIssueSearchOpen } from "@/lib/issue-search"
import { useEntityRefRow } from "./use-entity-ref-row"
import { useEntityRefTargets } from "./use-entity-ref-target"

// EXP-920: the hover card / tap sheet behind a tool-row chip, resolved per
// kind from THIS client's synced rows — never the server. One component per
// kind (each owns its live queries), one dispatcher. An issue reuses the
// issue preview outright (EXP-760); the row-less kinds (`repository`,
// `thread`) draw a slim card from the ref's own title; a `list` lists its
// members. An unsynced row renders nothing (the chip is muted and inert).
//
// The chrome is `EntityPreviewCard` in @exp/ui — the same ladder the desktop
// (`entity_preview.rs`), iOS (`EntityPreviewCard.swift`) and Android
// (`EntityPreviewCard.kt`) draw.

/** How many rows a board's or a list's card lists before "+N more". */
const CARD_ROWS_MAX = 6

function kindGlyph(ref: EntityRef, className = `size-3.5`): ReactNode {
  const Icon = conceptIcon(entityRefIcon(ref) as IconConcept)
  return <Icon className={className} />
}

function nounEyebrow(kind: string): string {
  const noun = entityKindNoun(kind, 1)
  return noun.charAt(0).toUpperCase() + noun.slice(1)
}

function moreLine(total: number, shown: number): string | null {
  const rest = total - shown
  return rest > 0 ? `+${rest} more` : null
}

/** The dispatcher. `members` = a `list` ref's absorbed member refs. */
export function EntityRefPreviewCard({
  entityRef,
  members,
}: {
  entityRef: EntityRef
  members: readonly EntityRef[]
}): ReactNode {
  const { row, synced } = useEntityRefRow(entityRef)
  if (!synced) return null
  switch (entityRef.kind) {
    case `issue`:
      return <IssuePreviewCard issueId={(row as ResolvedIssueRef).id} />
    case `board`:
      return <BoardCard board={row as Board} />
    case `action`:
      return <ActionCard action={row as Action} />
    case `automation`:
      return <AutomationCard automation={row as Automation} />
    case `comment`:
      return <CommentCard comment={row as Comment} />
    case `session`:
      return <SessionCard session={row as CodingSession} />
    case `label`:
      return <LabelCard label={row as Label} />
    case `status`:
      return <StatusCard status={row as StatusRowOption} />
    case `workflow`:
      return <WorkflowCard workflow={row as SyncedWorkflow} />
    case `device`:
      return <DeviceCard device={row as Device} />
    case `member`:
      return <MemberCard user={row as User} />
    case `team`:
      return <TeamCard team={row as Team} />
    case `invite`:
      return <InviteCard invite={row as TeamInvite} />
    case `notification`:
      return <NotificationCard notification={row as Notification} />
    case `attachment`:
      return <AttachmentCard attachment={row as Attachment} />
    case `repository`:
    case `thread`:
      return <SlimCard entityRef={entityRef} />
    case `list`:
      return <ListCard entityRef={entityRef} members={members} />
  }
  return null
}

// ── Issue rows (a board's open issues, a list's issue members) ──────────────

function issueRow(
  issue: Pick<ResolvedIssueRef, `id` | `identifier` | `title`> & {
    status: string
    statusId: string | null
  },
  resolveStatus: (issue: { status: string; statusId: string | null }) => StatusRowOption,
  link: EntityPreviewRow[`link`]
): EntityPreviewRow {
  const status = resolveStatus(issue)
  return {
    key: issue.id,
    icon: (
      <StatusGlyph
        icon={status.icon}
        colorClass={statusColorClass(status)}
        colorHex={status.builtinKey ? undefined : status.colorHex}
      />
    ),
    identifier: issue.identifier,
    primary: issue.title.trim() || `Untitled issue`,
    link,
  }
}

// ── Board ───────────────────────────────────────────────────────────────────

function BoardCard({ board }: { board: Board }) {
  const { resolve } = useTeamStatusesContext()
  const { data: issueRows } = useLiveQuery(
    (query) =>
      query
        .from({ issues: issueCollection })
        .where(({ issues }) => eq(issues.boardId, board.id)),
    [board.id]
  )
  const open = useMemo(
    () =>
      ((issueRows ?? []) as Issue[])
        .filter((issue) => isIssueSearchOpen(issue.status))
        .sort(
          (left, right) =>
            new Date(right.updatedAt).getTime() - new Date(left.updatedAt).getTime()
        ),
    [issueRows]
  )
  const shown = useMemo(() => open.slice(0, CARD_ROWS_MAX), [open])
  const refs = useMemo<EntityRef[]>(
    () => shown.map((issue) => ({ kind: `issue`, id: issue.id, identifier: issue.identifier })),
    [shown]
  )
  const target = useEntityRefTargets(refs)
  const rows = shown.map((issue, index) =>
    issueRow(issue, resolve, target(refs[index]!).link)
  )
  const count = open.length
  return (
    <EntityPreviewCard
      icon={<BoardGlyph board={board} className="size-3.5" />}
      eyebrow={`Board · ${board.prefix}`}
      title={board.name}
      subtitle={
        count === 0
          ? `No open issues`
          : `${count} open ${entityKindNoun(`issue`, count)}`
      }
      rows={rows}
      more={moreLine(count, shown.length)}
      testId="entity-preview-board"
    />
  )
}

// ── Action ──────────────────────────────────────────────────────────────────

function ActionCard({ action }: { action: Action }) {
  const Icon = getActionIcon(action)
  const repos = useTeamRepos(action.teamId, action.repositoryId !== null)
  const repo = repos?.find((row) => row.id === action.repositoryId) ?? null
  const inputs = Array.isArray(action.inputs) ? action.inputs.length : 0
  return (
    <EntityPreviewCard
      icon={<Icon className="size-3.5" />}
      eyebrow={nounEyebrow(`action`)}
      title={action.name}
      body={action.description}
      facts={
        <>
          {action.repositoryId && (
            <Pill size="sm" leading={kindGlyph({ kind: `repository`, id: `` }, `size-3`)}>
              {repo?.fullName ?? `Repository`}
            </Pill>
          )}
          <Pill size="sm">{`${inputs} ${inputs === 1 ? `input` : `inputs`}`}</Pill>
        </>
      }
      testId="entity-preview-action"
    />
  )
}

// ── Automation ──────────────────────────────────────────────────────────────

function AutomationCard({ automation }: { automation: Automation }) {
  const { data: actionRows } = useLiveQuery(
    (query) =>
      query
        .from({ actions: actionCollection })
        .where(({ actions }) => eq(actions.id, automation.actionId)),
    [automation.actionId]
  )
  // The `actions` shape drops `body`; the card only needs the name.
  const action = (actionRows?.[0] ?? null) as unknown as Pick<Action, `name`> | null
  const { data: deviceRows } = useLiveQuery(
    (query) =>
      query
        .from({ devices: deviceCollection })
        .where(({ devices }) => eq(devices.deviceId, automation.deviceId)),
    [automation.deviceId]
  )
  const device = (deviceRows?.[0] ?? null) as Device | null
  const trigger = parseAutomationTrigger(automation.trigger)
  return (
    <EntityPreviewCard
      icon={kindGlyph({ kind: `automation`, id: `` })}
      eyebrow={nounEyebrow(`automation`)}
      title={action ? `Runs ${action.name}` : `Runs an action`}
      subtitle={trigger ? triggerSummary(trigger) : null}
      facts={
        <>
          <Pill
            size="sm"
            leading={<LiveDot tone={automation.enabled ? `live` : `idle`} className="size-1.5" />}
          >
            {automation.enabled ? `Enabled` : `Disabled`}
          </Pill>
          {device?.label && (
            <Pill size="sm" leading={kindGlyph({ kind: `device`, id: `` }, `size-3`)}>
              {device.label}
            </Pill>
          )}
        </>
      }
      testId="entity-preview-automation"
    />
  )
}

// ── Comment ─────────────────────────────────────────────────────────────────

function CommentCard({ comment }: { comment: Comment }) {
  const issueRefs = useIssueRefs()
  const authorId = comment.authorId
  const { data: userRows } = useLiveQuery(
    (query) =>
      authorId
        ? query.from({ users: userCollection }).where(({ users }) => eq(users.id, authorId))
        : undefined,
    [authorId]
  )
  const author = (userRows?.[0] ?? null) as User | null
  const name = displayUserName(author ?? undefined, authorId ?? undefined)
  const issue = issueRefs?.resolveById(comment.issueId) ?? null
  const when = relativeTime(comment.createdAt)
  return (
    <EntityPreviewCard
      icon={
        <UserAvatar
          size={16}
          user={{ id: authorId, name, image: author?.image }}
        />
      }
      eyebrow={when ? `${name} · ${when}` : name}
      title={nounEyebrow(`comment`)}
      body={comment.body}
      footer={
        issue ? (
          <div>
            <IssueChip
              issue={issue}
              preview={false}
              onClick={() => issueRefs?.open(issue.identifier)}
            />
          </div>
        ) : null
      }
      testId="entity-preview-comment"
    />
  )
}

// ── Session ─────────────────────────────────────────────────────────────────

const SESSION_TONE: Record<string, LiveDotTone> = {
  running: `live`,
  review: `live`,
  needs_input: `attention`,
  done: `done`,
}

function SessionCard({ session }: { session: CodingSession }) {
  const issueRefs = useIssueRefs()
  const issue = session.issueId ? issueRefs?.resolveById(session.issueId) ?? undefined : undefined
  const identity = sessionIdentity({
    session,
    issue,
    batchIssues: issueRefs?.rows ?? [],
  })
  const device = useSessionDevice(session)
  const state = sessionDisplayState(session, session.prState)
  const ended = session.status === `ended`
  const paused = sessionIsPaused(state, device)
  const line = sessionStatusLine({
    state,
    paused,
    device: device.label ?? `Unknown device`,
    startedAt: session.startedAt,
  })
  const tone: LiveDotTone = ended ? `muted` : (SESSION_TONE[state] ?? `muted`)
  const started = relativeTime(session.startedAt)
  return (
    <EntityPreviewCard
      icon={<AgentBrandMark agent={session.agent} className="size-3.5" />}
      eyebrow={identity.identifier ? `Run · ${identity.identifier}` : `Run`}
      title={identity.subject}
      subtitle={ended && started ? `Ended · ${device.label ?? `Unknown device`} · started ${started}` : line.text}
      facts={
        <Pill size="sm" leading={<LiveDot tone={tone} className="size-1.5" />}>
          {ended
            ? `Ended`
            : state === `needs_input`
              ? `Needs input`
              : state === `review`
                ? `In review`
                : state === `done`
                  ? `Done`
                  : `Running`}
        </Pill>
      }
      testId="entity-preview-session"
    />
  )
}

// ── Label / status ──────────────────────────────────────────────────────────

function LabelCard({ label }: { label: Label }) {
  return (
    <EntityPreviewCard
      icon={
        <span
          aria-hidden
          className="size-2.5 rounded-full"
          style={{ backgroundColor: label.color }}
        />
      }
      eyebrow={nounEyebrow(`label`)}
      title={label.name}
      testId="entity-preview-label"
    />
  )
}

const CATEGORY_LABEL: Record<string, string> = {
  backlog: `Backlog`,
  unstarted: `Unstarted`,
  started: `Started`,
  completed: `Completed`,
  cancelled: `Cancelled`,
  duplicate: `Duplicate`,
}

function StatusCard({ status }: { status: StatusRowOption }) {
  return (
    <EntityPreviewCard
      icon={<StatusIcon option={status} className="!h-3.5 !w-3.5" />}
      eyebrow={nounEyebrow(`status`)}
      title={status.name}
      subtitle={CATEGORY_LABEL[status.category] ?? status.category}
      testId="entity-preview-status"
    />
  )
}

// ── Workflow ────────────────────────────────────────────────────────────────

function WorkflowCard({ workflow }: { workflow: SyncedWorkflow }) {
  const status = workflow.status
  return (
    <EntityPreviewCard
      icon={kindGlyph({ kind: `workflow`, id: `` })}
      eyebrow={nounEyebrow(`workflow`)}
      title={workflow.name}
      subtitle={workflowRowSubtitle(status, workflow.metrics)}
      facts={
        <Pill size="sm">{status.charAt(0).toUpperCase() + status.slice(1)}</Pill>
      }
      testId="entity-preview-workflow"
    />
  )
}

// ── Device ──────────────────────────────────────────────────────────────────

function DeviceCard({ device }: { device: Device }) {
  const now = useNow(30_000)
  const online = deviceRowIsOnline(device.lastSeenAt, now)
  const Icon = getDeviceIcon(device)
  return (
    <EntityPreviewCard
      icon={<Icon className="size-3.5" />}
      eyebrow={nounEyebrow(`device`)}
      title={device.label}
      subtitle={device.platform ?? null}
      facts={
        <Pill size="sm" leading={<LiveDot tone={online ? `live` : `idle`} className="size-1.5" />}>
          {online ? `Online` : `Offline`}
        </Pill>
      }
      testId="entity-preview-device"
    />
  )
}

// ── Member / team / invite ──────────────────────────────────────────────────

function MemberCard({ user }: { user: User }) {
  const issueRefs = useIssueRefs()
  const teamId = issueRefs?.teamId
  const { data: memberRows } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ members: teamMemberCollection })
            .where(({ members }) =>
              and(eq(members.teamId, teamId), eq(members.userId, user.id))
            )
        : undefined,
    [teamId, user.id]
  )
  const member = (memberRows?.[0] ?? null) as TeamMember | null
  const name = displayUserName(user, user.id)
  return (
    <EntityPreviewCard
      icon={<UserAvatar size={16} user={user} />}
      eyebrow={nounEyebrow(`member`)}
      title={name}
      subtitle={user.email}
      facts={member ? <Pill size="sm">{member.role === `owner` ? `Owner` : `Member`}</Pill> : null}
      testId="entity-preview-member"
    />
  )
}

function TeamCard({ team }: { team: Team }) {
  return (
    <EntityPreviewCard
      icon={<TeamAvatar name={team.name} size={14} />}
      eyebrow={nounEyebrow(`team`)}
      title={team.name}
      subtitle={team.slug}
      testId="entity-preview-team"
    />
  )
}

function InviteCard({ invite }: { invite: TeamInvite }) {
  const accepted = invite.acceptedAt !== null
  const expired = !accepted && new Date(invite.expiresAt).getTime() < Date.now()
  return (
    <EntityPreviewCard
      icon={kindGlyph({ kind: `invite`, id: `` })}
      eyebrow={nounEyebrow(`invite`)}
      title={invite.email ?? `Invite link`}
      facts={
        <>
          <Pill size="sm">{invite.role === `owner` ? `Owner` : `Member`}</Pill>
          <Pill size="sm">{accepted ? `Accepted` : expired ? `Expired` : `Pending`}</Pill>
        </>
      }
      testId="entity-preview-invite"
    />
  )
}

// ── Notification / attachment ───────────────────────────────────────────────

function NotificationCard({ notification }: { notification: Notification }) {
  const when = relativeTime(notification.createdAt)
  return (
    <EntityPreviewCard
      icon={kindGlyph({ kind: `notification`, id: `` })}
      eyebrow={when ? `${nounEyebrow(`notification`)} · ${when}` : nounEyebrow(`notification`)}
      title={notification.title}
      body={notification.body}
      facts={notification.readAt ? null : <Pill size="sm" leading={<LiveDot tone="unread" className="size-1.5" />}>Unread</Pill>}
      testId="entity-preview-notification"
    />
  )
}

function AttachmentCard({ attachment }: { attachment: Attachment }) {
  return (
    <EntityPreviewCard
      icon={kindGlyph({ kind: `attachment`, id: `` })}
      eyebrow={nounEyebrow(`attachment`)}
      title={attachment.filename}
      subtitle={`${formatAttachmentSize(attachment.sizeBytes)} · ${attachment.contentType}`}
      testId="entity-preview-attachment"
    />
  )
}

// ── Row-less kinds ──────────────────────────────────────────────────────────

/** A repository or a helpdesk thread: server-only rows, so the card is the
 *  ref's own title under its kind — nothing to resolve. */
function SlimCard({ entityRef }: { entityRef: EntityRef }) {
  const title = entityRef.title?.trim()
  if (!title) return null
  return (
    <EntityPreviewCard
      icon={kindGlyph(entityRef)}
      eyebrow={nounEyebrow(entityRef.kind)}
      title={title}
      testId={`entity-preview-${entityRef.kind}`}
    />
  )
}

// ── List ────────────────────────────────────────────────────────────────────

/** A list chip's card: the chip label as the title, one row per member ref
 *  (an issue with its resolved status glyph and identifier, anything else
 *  its kind glyph and label), each a target, and "+N more" when the answer
 *  carried more rows than the refs it named. */
function ListCard({
  entityRef,
  members,
}: {
  entityRef: EntityRef
  members: readonly EntityRef[]
}) {
  const issueRefs = useIssueRefs()
  const { resolve } = useTeamStatusesContext()
  const target = useEntityRefTargets(members)
  const rows: EntityPreviewRow[] = members.map((member, index) => {
    const { link } = target(member)
    if (member.kind === `issue`) {
      const issue =
        issueRefs?.resolveById(member.id) ??
        (member.identifier ? issueRefs?.resolve(member.identifier) : null) ??
        null
      if (issue) return issueRow(issue, resolve, link)
      return {
        key: `${member.kind}:${member.id}:${index}`,
        icon: kindGlyph(member, `size-3.5 text-muted-foreground`),
        identifier: member.identifier ?? null,
        primary: member.title?.trim() || (member.identifier ? `` : `Issue`),
        link,
      }
    }
    return {
      key: `${member.kind}:${member.id}:${index}`,
      icon: kindGlyph(member),
      primary: entityChipLabel(member),
      link,
    }
  })
  const count = Math.max(0, Math.round(entityRef.count ?? 0))
  return (
    <EntityPreviewCard
      icon={kindGlyph(entityRef)}
      eyebrow={nounEyebrow(`list`)}
      title={entityChipLabel(entityRef)}
      rows={rows}
      more={moreLine(count, members.length)}
      testId="entity-preview-list"
    />
  )
}
