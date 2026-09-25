import { useMemo, useState } from "react"
import { Link, useParams } from "@tanstack/react-router"
import {
  conceptIcon,
  ListRow,
  TREE_BASE,
  TREE_INDENT,
  TreeGuides,
  treeGuides,
  type TreeGuide,
} from "@exp/ui"
import type { Board, CodingSession, Issue } from "@/db/schema"
import type { SessionDevice } from "@/lib/session-device"
import { runHasEnded } from "@/lib/past-runs"
import { sessionIdentity } from "@/lib/session-identity"
import {
  sessionTree,
  sessionTreeNodeKey,
  visibleSessionTreeRows,
  type SessionTreeContext,
  type SessionTreeFlatRow,
  type SessionTreeNode,
} from "@/lib/sessions/session-tree"
import { cn } from "@/lib/utils"
import { pastRunRowByline } from "@/components/agent-session-row"
import {
  PastSessionRow,
  RunningSessionRow,
} from "@/components/session-list-rows"
import type {
  SessionListRow,
  SessionMergeTarget,
} from "@/hooks/use-agents-data"
import { useTeamBySlug } from "@/hooks/use-team-data"
import {
  useTeamWorkflowNodes,
  useTeamWorkflows,
} from "@/hooks/use-workflows"

// EXP-996: the session list as a TREE, not a flat roll of strangers.
//
// EXP-818 already nested a child run under its parent. What it could not say
// is that a dozen rows are ONE thing: the nodes of a workflow read as a dozen
// unrelated runs, and so did a stack, and a resumed run read as two. So the
// list draws the `sessionTree` selector (`lib/sessions/session-tree.ts`, the
// ×4 rule): a resume succession is ONE row, children nest, a workflow's runs
// sit under one group row that LINKS to the workflow, and a stack sits under
// one group row in linear order, lowest first. Stacks and workflows are NOT
// unified (the EXP-996 decision) — a stack is a linear group with its own
// icon, and that icon is the whole difference the reader needs.
//
// A group row is not a run: no state dot, no device, no kill. It only names
// what the rows below it belong to and folds them away.

const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)
const WorkflowIcon = conceptIcon(`nav-workflows`)
const StackIcon = conceptIcon(`pr-stack`)

/** EXP-996: one group row's fold labels. The session rows' own pair
 *  (`Collapse child runs`) says CHILD RUNS, which a group has none of. */
export const COLLAPSE_GROUP_LABEL = `Collapse these runs`
export const EXPAND_GROUP_LABEL = `Expand these runs`
/** What a stack group row is called. A workflow group wears its own name. */
export const STACK_GROUP_LABEL = `Stacked pull requests`

/** What this list needs of a row. `SessionListRow` (the joined running rows)
 *  and `PastRunRow` (Recent's rows, which carry their own title) both satisfy
 *  it, so one component serves every band. */
export interface TreeListRow {
  session: CodingSession
  issue: Issue | undefined
  /** EXP-876: a batch row's covered issues — what names it. */
  batchIssues?: Issue[]
  board: Board | undefined
  device: SessionDevice
  paused?: boolean
  mergeTarget?: SessionMergeTarget
  /** Recent's precomputed title; absent = derive it from the identity. */
  title?: string
  /** Recent's precomputed lead-in; `undefined` = derive it from the identity
   *  (which is where a batch's `EXP-874 +2` comes from). */
  identifier?: string | null
}

/**
 * EXP-996: the grouping context, resolved ONCE per surface.
 *
 * The rows alone cannot say which workflow a run belongs to (a `workflow_nodes`
 * join) nor which issues stack on which (`issues.pr_base_branch`). The stack
 * edges come from the LISTED rows' own issues: a stack only becomes a group
 * when two of its runs are listed, so an unlisted lower member would change
 * nothing but the group's root — and a root nobody can see is worse than the
 * lowest one they can.
 */
export function useSessionTreeContext(
  teamId: string | undefined,
  rows: readonly TreeListRow[]
): SessionTreeContext {
  const workflows = useTeamWorkflows(teamId)
  const nodes = useTeamWorkflowNodes(teamId)
  const issues = useMemo(() => {
    const byId = new Map<string, Issue>()
    for (const row of rows) {
      if (row.issue) byId.set(row.issue.id, row.issue)
      for (const issue of row.batchIssues ?? []) byId.set(issue.id, issue)
    }
    return [...byId.values()]
  }, [rows])
  return useMemo(
    () => ({
      workflows: workflows.map((workflow) => ({
        id: workflow.id,
        name: workflow.name,
        status: workflow.status,
      })),
      workflowNodes: nodes.map((node) => ({
        workflowId: node.workflowId,
        issueId: node.issueId,
        sessionId: node.sessionId,
      })),
      issues,
    }),
    [workflows, nodes, issues]
  )
}

/** EXP-996: the VISIBLE rows of a session tree, with the EXP-965 connector
 *  geometry — the one model behind every drawn list. The connector reads off
 *  the visible depths, so a folded subtree simply is not there. */
export function useSessionTreeRows<T extends TreeListRow>(
  rows: readonly T[],
  context: SessionTreeContext,
  collapsed: ReadonlySet<string>
): { flat: SessionTreeFlatRow<CodingSession>; row: T | undefined; guide: TreeGuide }[] {
  return useMemo(() => {
    const byId = new Map(rows.map((row) => [row.session.id, row]))
    const visible = visibleSessionTreeRows(
      sessionTree(
        rows.map((row) => row.session),
        context
      ),
      collapsed
    )
    const guides = treeGuides(visible.map((entry) => entry.depth))
    return visible.map((flat, index) => ({
      flat,
      row:
        flat.node.kind === `session`
          ? byId.get(flat.node.session.id)
          : undefined,
      guide: guides[index]!,
    }))
  }, [rows, context, collapsed])
}

/** EXP-996: the collapse set every session list holds — expanded by default,
 *  per node, for as long as the list is mounted, keyed by the node key so a
 *  group folds exactly like a parent run does. */
export function useCollapsedNodes(): [
  ReadonlySet<string>,
  (key: string) => void,
] {
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(
    () => new Set<string>()
  )
  const toggle = (key: string) =>
    setCollapsed((current) => {
      const next = new Set(current)
      if (!next.delete(key)) next.add(key)
      return next
    })
  return [collapsed, toggle]
}

/** The fold chevron a group row and a session row share. */
export function TreeFoldToggle({
  expanded,
  label,
  onToggle,
}: {
  expanded: boolean
  label: string
  onToggle: () => void
}) {
  return (
    <span
      role="button"
      tabIndex={0}
      aria-label={label}
      aria-expanded={expanded}
      className="flex shrink-0 items-center justify-center text-muted-foreground hover:text-foreground focus-visible:text-foreground focus-visible:outline-none"
      onClick={(event) => {
        event.stopPropagation()
        onToggle()
      }}
      // A `role="button"` span gets none of a button's keys for free: Enter
      // and Space fold, and stay off the row underneath (its link, its open).
      onKeyDown={(event) => {
        if (event.key !== `Enter` && event.key !== ` `) return
        event.preventDefault()
        event.stopPropagation()
        onToggle()
      }}
    >
      {expanded ? (
        <ChevronDownIcon className="size-3" />
      ) : (
        <ChevronRightIcon className="size-3" />
      )}
    </span>
  )
}

/**
 * EXP-996: the group row — a workflow's runs, or a stack's.
 *
 * The whole row folds; only the workflow's NAME links onward, because a stack
 * is not a place you can go (its members are its only page). The count is the
 * row's own fact: a group is its children, so how many there are is what the
 * reader is deciding to fold away.
 */
export function SessionGroupRow({
  node,
  depth,
  guide,
  expanded,
  onToggle,
  teamSlug,
  className,
}: {
  node: Extract<SessionTreeNode<CodingSession>, { kind: `workflow` | `stack` }>
  depth: number
  guide?: TreeGuide | null
  expanded: boolean
  onToggle: () => void
  /** Absent = the workflow's name does not link (the styleguide, a surface
   *  with no team in its URL). */
  teamSlug?: string
  className?: string
}) {
  const workflow = node.kind === `workflow` ? node : null
  const Icon = workflow ? WorkflowIcon : StackIcon
  const name = workflow ? workflow.name : STACK_GROUP_LABEL
  return (
    <ListRow
      interactive
      onClick={onToggle}
      className={cn(`relative h-8 gap-1.5 py-0 pr-2 text-sm`, className)}
      style={{ paddingLeft: `${TREE_BASE + depth * TREE_INDENT}px` }}
      data-testid={`session-group-${sessionTreeNodeKey(node)}`}
    >
      <TreeGuides guide={guide ?? null} />
      <TreeFoldToggle
        expanded={expanded}
        label={expanded ? COLLAPSE_GROUP_LABEL : EXPAND_GROUP_LABEL}
        onToggle={onToggle}
      />
      <Icon className="size-3.5 shrink-0 text-muted-foreground" />
      {workflow && teamSlug ? (
        <Link
          to="/t/$teamSlug/workflows/$workflowId"
          params={{ teamSlug, workflowId: workflow.workflowId }}
          className="min-w-0 flex-1 truncate font-medium hover:underline"
          onClick={(event) => event.stopPropagation()}
        >
          {name}
        </Link>
      ) : (
        <span className="min-w-0 flex-1 truncate font-medium">{name}</span>
      )}
      <span className="shrink-0 font-mono text-xs text-muted-foreground">
        {node.children.length}
      </span>
    </ListRow>
  )
}

// EXP-897: the ONE nested session list. EXP-818 gave the Agent page's Running
// band the tree; every OTHER list (the Agent page's Recent band, the sidebar's
// automated-runs nav, the Automations tab's "Recent automated runs") still
// listed a child run as a stranger. They all render this component now, so a
// run started by another run reads the same wherever it is listed — the ×4
// rule (desktop `sessions_section`, iOS `AgentSessionsList`, Android
// `AgentSessionsList.kt`).
//
// The CAP belongs to the caller, applied to the ROWS before they get here
// (`PAST_RUN_CAP`, the Automations tab's `slice(0, 10)`): a tree built from a
// truncated list simply leaves an unlisted parent's child a root, which is
// exactly rule 6 of `lib/sessions/session-tree.ts`.

/** EXP-996: the team the list is being read IN. Every session list lives under
 *  `t/$teamSlug`, so the workflow join and the group row's link need no prop
 *  threaded through four call sites. */
function useListTeam(teamId?: string, teamSlug?: string) {
  const params = useParams({ strict: false }) as { teamSlug?: string }
  const slug = teamSlug ?? params.teamSlug
  const team = useTeamBySlug(slug ?? ``)
  return { teamId: teamId ?? team?.id, teamSlug: slug }
}

export function SessionTree({
  rows,
  teamId: teamIdProp,
  teamSlug: teamSlugProp,
  activeSessionId = null,
  onOpen,
  emptyNote,
  titleOf,
}: {
  /** The rows to nest, in the caller's order. */
  rows: readonly TreeListRow[]
  /** Scopes the workflow join; absent = read off the route. */
  teamId?: string
  /** Lets a workflow group row link onward; absent = read off the route. */
  teamSlug?: string
  activeSessionId?: string | null
  onOpen: (session: CodingSession) => void
  /** Shown instead of the rows when there are none. Absent = render nothing. */
  emptyNote?: string
  /** A row's title when the caller knows better than the identity (the
   *  Automations tab resolves a deleted action's live name). */
  titleOf?: (row: TreeListRow) => string
}) {
  const [collapsed, toggle] = useCollapsedNodes()
  const { teamId, teamSlug } = useListTeam(teamIdProp, teamSlugProp)
  const context = useSessionTreeContext(teamId, rows)
  const tree = useSessionTreeRows(rows, context, collapsed)

  if (tree.length === 0) {
    return emptyNote ? (
      <div className="px-3 py-2 text-xs text-muted-foreground">{emptyNote}</div>
    ) : null
  }

  return (
    // Gapless (EXP-965: nothing for a connector to bridge).
    <div className="flex flex-col">
      {tree.map(({ flat, row, guide }) => {
        const { node, key, depth, hasChildren } = flat
        const expanded = !collapsed.has(key)
        if (node.kind !== `session`) {
          return (
            <SessionGroupRow
              key={key}
              node={node}
              depth={depth}
              guide={guide}
              expanded={expanded}
              onToggle={() => toggle(key)}
              teamSlug={teamSlug}
            />
          )
        }
        if (!row) return null
        const session = node.session
        const active = session.id === activeSessionId
        return runHasEnded(session) ? (
          <PastSessionRow
            key={key}
            sessionId={session.id}
            title={titleOf?.(row) ?? row.title ?? sessionIdentity(row).subject}
            identifier={row.identifier ?? sessionIdentity(row).identifier}
            byline={pastRunRowByline(row)}
            depth={depth}
            guide={guide}
            active={active}
            expandable={hasChildren}
            expanded={expanded}
            onToggle={() => toggle(key)}
            onOpen={() => onOpen(session)}
          />
        ) : (
          <RunningSessionRow
            key={key}
            row={{ ...row, paused: row.paused ?? false } as SessionListRow}
            depth={depth}
            guide={guide}
            active={active}
            expandable={hasChildren}
            expanded={expanded}
            onToggle={() => toggle(key)}
            onOpen={() => onOpen(session)}
          />
        )
      })}
    </div>
  )
}
