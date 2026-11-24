import { useEffect, useRef } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import { toast } from "@exp/ui"
import { inboxRowTarget } from "@exp/domain-contract/inbox-route"
import {
  INBOX_THREAD,
  NOTIFICATION_DELIVERY_COPY,
  notificationPresentation,
  notificationThread,
} from "@exp/domain-contract/notification-delivery"
import {
  boardCollection,
  issueCollection,
  notificationCollection,
  sessionCollection,
  teamCollection,
} from "@/lib/collections"
import type { Notification } from "@/db/schema"
import { chatTeamSlug } from "@/lib/chat-teams"

// #EXP-1815 (notification-delivery.json PRESENTATION): web has no system
// channel; a NEW unread inbox row raises THE toast (kind info, the row's
// title + body, `Open` = the inbox tap) unless the page already shows it
// (the row's thread or the Inbox) or its source is muted (`pushed_at`
// NULL). Like the desktop's gate: the first ready snapshot is the backlog
// and never toasts, and a row older than `FRESH_MS` (a late sync after a
// reconnect) stays quiet too.

const FRESH_MS = 2 * 60_000

/** The thread the current page shows (null = a page without one). */
export function viewingThread(pathname: string): string | null {
  const parts = pathname.split(`/`).filter(Boolean)
  if (parts[0] !== `t` || parts.length < 3) return null
  const [, , section, a, b, c] = parts
  if (section === `inbox`) return INBOX_THREAD
  if (section === `sessions` && a) {
    const session = sessionCollection.get(a)
    return `session:${session?.rootId ?? a}`
  }
  const identifier = section === `issues` ? a : section === `boards` && b === `issues` ? c : null
  if (identifier) {
    const issue = issueCollection.toArray.find((row) => row.identifier.toLowerCase() === identifier.toLowerCase())
    return issue ? `issue:${issue.id}` : `issue-ref:${identifier}`
  }
  return null
}

/** The team slug of a `/t/<slug>/…` path (a team-less page has none). */
function routeTeamSlug(pathname: string): string | null {
  const parts = pathname.split(`/`).filter(Boolean)
  return parts[0] === `t` && parts[1] ? decodeURIComponent(parts[1]) : null
}

export function rowThread(row: Notification): string {
  const session = row.sessionId ? sessionCollection.get(row.sessionId) : undefined
  return notificationThread({
    type: row.type,
    issueId: row.issueId ?? null,
    sessionRootId: row.sessionId ? (session?.rootId ?? row.sessionId) : null,
    deviceId: row.deviceId ?? null,
  })
}

function teamSlugOf(row: Notification): string | null {
  if (row.teamId) return teamCollection.get(row.teamId)?.slug ?? null
  const issue = row.issueId ? issueCollection.get(row.issueId) : undefined
  const board = issue ? boardCollection.get(issue.boardId) : undefined
  return board ? (teamCollection.get(board.teamId)?.slug ?? null) : null
}

export function useNotificationToasts(userId: string | null | undefined): void {
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const pathRef = useRef(pathname)
  pathRef.current = pathname
  const { data: rows, isReady } = useLiveQuery(
    (query) => (userId ? query.from({ n: notificationCollection }) : undefined),
    [userId]
  )
  const seen = useRef<Set<string> | null>(null)

  useEffect(() => {
    seen.current = null
  }, [userId])

  useEffect(() => {
    if (!isReady || !rows) return
    if (!seen.current) {
      seen.current = new Set(rows.map((row) => row.id))
      return
    }
    for (const row of rows as Notification[]) {
      if (seen.current.has(row.id)) continue
      seen.current.add(row.id)
      if (row.readAt) continue
      if (Date.now() - new Date(row.createdAt).getTime() > FRESH_MS) continue
      const presentation = notificationPresentation({
        platform: `web`,
        active: typeof document === `undefined` || document.visibilityState === `visible`,
        viewing: viewingThread(pathRef.current),
        thread: rowThread(row),
        muted: !row.pushedAt,
      })
      if (presentation === `none`) continue
      toast.info(row.title, {
        description: row.body ?? undefined,
        action: {
          label: NOTIFICATION_DELIVERY_COPY.toastAction,
          onClick: () => openRow(row),
        },
      })
    }

    function openRow(row: Notification) {
      const teamSlug = teamSlugOf(row)
      const target = inboxRowTarget({
        type: row.type,
        issueId: row.issueId ?? null,
        sessionId: row.sessionId ?? null,
        deviceId: row.deviceId ?? null,
      })
      if (target.kind === `issue` && teamSlug) {
        const identifier = issueCollection.get(target.issueId)?.identifier
        if (identifier) {
          void navigate({ to: `/t/$teamSlug/issues/$identifier`, params: { teamSlug, identifier } })
          return
        }
      }
      // #EXP-1987: a session of a team the viewer does not sync (the
      // Assistant's chat in their personal team) opens from the current team.
      // #EXP-2079 (`assistant.chatTeams` (7)): on a team page that team, on
      // a team-less page (Home, the account settings) the first chat team.
      const slug = teamSlug ?? routeTeamSlug(pathRef.current) ?? chatTeamSlug(target.kind === `session` ? target.sessionId : null)
      if (target.kind === `session` && slug) {
        void navigate({ to: `/t/$teamSlug/sessions/$sessionId`, params: { teamSlug: slug, sessionId: target.sessionId } })
        return
      }
      if (slug) void navigate({ to: `/t/$teamSlug/inbox`, params: { teamSlug: slug } })
    }
  }, [rows, isReady, navigate])
}
