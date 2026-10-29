/**
 * Resolve the seeded demo instance's real ids (EXP-566/EXP-627).
 *
 * The view catalog names its targets by PLACEHOLDER — `issue:$APP-5`,
 * `$emptyBoard`, `/support/$supportToken` — because none of those ids exist
 * until `seed:screenshots` has run, and every re-seed rotates
 * them (the users are recreated so the user-scoped Electric shapes get fresh
 * identities). This is the ONE lookup that turns the placeholders into ids, and
 * the only thing in the pipeline that talks to the database.
 *
 * `screenshot-ids.ts` is a thin printer over it (the desktop lane shells out to
 * `bun run screenshots:ids` and parses the JSON); `capture-views.ts` imports it
 * directly, lazily, and only when a route it actually wants carries a
 * placeholder — importing it eagerly would drag `@/db/connection` into every
 * browser capture run.
 *
 * Everything OPTIONAL here is optional for a reason, and a consumer that cannot
 * resolve one skips its view with a note rather than photographing a fallback
 * screen under the right filename.
 */
import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, boards, devices, issues, prompts, sessions, teams } from "@/db/schema"
import { mintReporterToken } from "@/lib/reporter/token"
import {
  DEMO_AGENT_ID,
  DEMO_DEVICE_ID,
  DEMO_DIFF_THREAD_ID,
  DEMO_GUIDE_THREAD_ID,
  DEMO_SESSION_ID,
  DEMO_THREAD_ID,
  DEMO_WIDGET_ISSUE_ID,
  EMPTY_BOARD_SLUG,
  TEAM_SLUG,
  DEMO_ACTION_ID,
} from "../screenshot-demo"

/**
 * The issues the Agent composer views chip (EXP-825), by placeholder. Two
 * codeable issues on the repo-backed board that no seeded session runs on (the
 * showcase run sits on APP-5): `$issueA` is APP-3, the issue the phone store
 * slide starts coding from, so the web `chat-issues` anchor names its chip;
 * `$issueB` is APP-6, a second open one assigned to the demo user. `$prIssue`
 * is APP-14, the issue behind the real review PR, so the fix-conflicts builtin
 * has an open PR to prefill.
 */
export const CHAT_ISSUE_IDENTIFIERS = {
  issueA: `APP-3`,
  issueB: `APP-6`,
  prIssue: `APP-14`,
} as const

/** The identifiers the catalog's desktop/native drives name by hand. */
export const WANTED_IDENTIFIERS = [`APP-3`, `APP-5`, `APP-6`, `APP-14`] as const

const RESEED = `Run \`bun run seed:screenshots\` first.`

export interface DemoIds {
  teamId: string
  boardId: string
  /** The seeded board with nothing on it — the `board-empty` view. */
  emptyBoardId?: string
  /** Keyed by human identifier: `{ "APP-5": "<uuid>" }`. */
  issues: Record<string, string>
  /** `$issueA` / `$issueB` / `$prIssue` — see `CHAT_ISSUE_IDENTIFIERS`. */
  issueAId: string
  issueBId: string
  prIssueId: string
  /** The widget-filed issue the reporter magic-link page is captured on
   *  (SLOP-4: pinned by the seed as `DEMO_WIDGET_ISSUE_ID`). */
  widgetIssueId?: string
  /**
   * The reporter's magic link for `widgetIssueId` — a CREDENTIAL. Never log
   * it, never write it into a file that gets committed: it grants anonymous
   * read/write on that conversation for as long as `BETTER_AUTH_SECRET`
   * lives. Omitted (not thrown) when the secret is unset.
   */
  supportToken?: string
  actionId?: string
  deviceId?: string
  /**
   * EXP-1293: the session-tree rows the views are photographed on, pinned by
   * the seed (screenshot-demo.ts) so the web routes and the desktop's
   * `EXP_DEV_SCREEN` can name them. Each is a presence check: absent = the
   * view skips rather than photographing a fallback screen.
   *
   *   sessionId        DEMO_SESSION_ID: the cold-start thread's child session
   *                    ("Profile the launch path"), the raw session view the
   *                    relay stub feeds (`session`, `steering`).
   *   steeredSessionId the same id under its pre-EXP-1293 name, which the
   *                    desktop lane's `$steeredSession` still reads.
   *   threadId         DEMO_THREAD_ID: "Reduce cold start below 800 ms".
   *   guideThreadId    DEMO_GUIDE_THREAD_ID: APP-15's thread with a Guide.
   *   diffThreadId     DEMO_DIFF_THREAD_ID: APP-14's thread (the stack top,
   *                    a real diff, the Stories tab's history).
   *   agentId          DEMO_AGENT_ID: the Triage agent.
   */
  sessionId?: string
  steeredSessionId?: string
  threadId?: string
  guideThreadId?: string
  diffThreadId?: string
  agentId?: string
}

export async function resolveDemoIds(): Promise<DemoIds> {
  const [team] = await db
    .select({ id: teams.id })
    .from(teams)
    .where(eq(teams.slug, TEAM_SLUG))
    .limit(1)
  if (!team) {
    throw new Error(`no team with slug "${TEAM_SLUG}" — the seed has not run. ${RESEED}`)
  }

  const [board] = await db
    .select({ id: boards.id })
    .from(boards)
    .where(and(eq(boards.teamId, team.id), eq(boards.slug, `mobile-app`)))
    .limit(1)
  if (!board) {
    throw new Error(`team "${TEAM_SLUG}" has no board "mobile-app". ${RESEED}`)
  }

  // The empty board is a nice-to-have: an older seed did not plant one, and a
  // desktop launched with an unresolvable EXP_DEV_BOARD_ID would just open the
  // last-visited board and photograph a FULL list under `board-empty`. So it is
  // reported as absent and the view skips.
  const [emptyBoard] = await db
    .select({ id: boards.id })
    .from(boards)
    .where(and(eq(boards.teamId, team.id), eq(boards.slug, EMPTY_BOARD_SLUG)))
    .limit(1)

  const issueRows = await db
    .select({ id: issues.id, identifier: issues.identifier })
    .from(issues)
    .where(
      and(eq(issues.boardId, board.id), inArray(issues.identifier, [...WANTED_IDENTIFIERS]))
    )
  const byIdentifier: Record<string, string> = {}
  for (const row of issueRows) {
    if (row.identifier) byIdentifier[row.identifier] = row.id
  }
  const missing = WANTED_IDENTIFIERS.filter((id) => !byIdentifier[id])
  if (missing.length > 0) {
    throw new Error(
      `board "mobile-app" is missing ${missing.join(`, `)} — the seed is stale or partial. ${RESEED}`
    )
  }

  // The widget-filed issue behind the reporter page: its id is PINNED by the
  // seed, so this is a presence check (an older seed planted helpdesk threads
  // instead — reported as absent so the view skips rather than photographing
  // the "conversation moved" card under its name).
  const [widgetIssue] = await db
    .select({ id: issues.id })
    .from(issues)
    .where(and(eq(issues.id, DEMO_WIDGET_ISSUE_ID), eq(issues.teamId, team.id)))
    .limit(1)

  // The prompt the `settings-prompt`-style drives open ("Fix flaky tests");
  // the seed pins its id (NEW-2: prompts replaced actions, the id keeps its
  // legacy name for the drives' `$action` placeholder).
  const [action] = await db
    .select({ id: prompts.id })
    .from(prompts)
    .where(and(eq(prompts.teamId, team.id), eq(prompts.id, DEMO_ACTION_ID)))
    .limit(1)
  if (!action) {
    throw new Error(`team "${TEAM_SLUG}" has no saved prompts. ${RESEED}`)
  }

  // The demo user's OWN machine, as announced by `screenshots:desktop` — pinned
  // by its steer device id, never "the oldest row". The seed also plants a
  // teammate's shared server, which is deliberately older, and an unfiltered
  // pick would hand the desktop lane a device that is not in the demo user's
  // synced shape at all (Device settings is own-devices-only, so the dialog
  // would silently never open). Absent until the stub has run once, hence no
  // throw.
  const [device] = await db
    .select({ id: devices.id })
    .from(devices)
    .where(eq(devices.deviceId, DEMO_DEVICE_ID))
    .limit(1)

  // EXP-1293: the pinned session-tree rows, presence-checked — an older seed
  // (or one that never planted the rows) leaves a route pointing at nothing,
  // and the app would photograph its fallback under the view's name.
  const pinned = await db
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        eq(sessions.teamId, team.id),
        inArray(sessions.id, [DEMO_SESSION_ID, DEMO_THREAD_ID, DEMO_GUIDE_THREAD_ID, DEMO_DIFF_THREAD_ID])
      )
    )
  // NEW-2: the demo agent is an `agents` row.
  const pinnedAgents = await db
    .select({ id: agents.id })
    .from(agents)
    .where(and(eq(agents.teamId, team.id), eq(agents.id, DEMO_AGENT_ID)))
  const present = new Set([...pinned, ...pinnedAgents].map((row) => row.id))
  const ifPresent = (id: string) => (present.has(id) ? id : undefined)

  return {
    teamId: team.id,
    boardId: board.id,
    emptyBoardId: emptyBoard?.id,
    issues: byIdentifier,
    issueAId: byIdentifier[CHAT_ISSUE_IDENTIFIERS.issueA]!,
    issueBId: byIdentifier[CHAT_ISSUE_IDENTIFIERS.issueB]!,
    prIssueId: byIdentifier[CHAT_ISSUE_IDENTIFIERS.prIssue]!,
    widgetIssueId: widgetIssue?.id,
    // `mintReporterToken` throws without BETTER_AUTH_SECRET; a capture host
    // that has not exported it should skip the reporter view, not fail the run.
    supportToken:
      widgetIssue && process.env.BETTER_AUTH_SECRET
        ? mintReporterToken(widgetIssue.id)
        : undefined,
    actionId: action.id,
    deviceId: device?.id,
    sessionId: ifPresent(DEMO_SESSION_ID),
    steeredSessionId: ifPresent(DEMO_SESSION_ID),
    threadId: ifPresent(DEMO_THREAD_ID),
    guideThreadId: ifPresent(DEMO_GUIDE_THREAD_ID),
    diffThreadId: ifPresent(DEMO_DIFF_THREAD_ID),
    agentId: ifPresent(DEMO_AGENT_ID),
  }
}
