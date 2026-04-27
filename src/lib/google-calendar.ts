import { and, eq } from "drizzle-orm"
import { calendar, type calendar_v3 } from "@googleapis/calendar"
import { OAuth2Client } from "google-auth-library"
import { db } from "@/db/connection"
import { accounts, issues, type Issue } from "@/db/schema"
import { auth } from "@/lib/auth"
import { getIssueDescriptionText } from "@/lib/domain"

const PROVIDER_ID = `google`
const CALENDAR_ID = `primary`

/**
 * Returns a Calendar client, or null if the user has not linked Google.
 * Throws if the account exists but the access token can't be obtained —
 * those errors must be surfaced (persisted to the issue) so the user can
 * see what's wrong instead of the sync silently no-op'ing forever.
 */
async function getCalendarClient(
  userId: string
): Promise<calendar_v3.Calendar | null> {
  const [account] = await db
    .select({ id: accounts.id })
    .from(accounts)
    .where(
      and(eq(accounts.userId, userId), eq(accounts.providerId, PROVIDER_ID))
    )
    .limit(1)

  if (!account) return null

  const result = await auth.api.getAccessToken({
    body: { providerId: PROVIDER_ID, userId },
  })
  const accessToken = result.accessToken ?? null
  if (!accessToken) {
    throw new Error(
      `Better Auth getAccessToken returned no access token (refresh token missing or revoked?)`
    )
  }

  const oauth = new OAuth2Client()
  oauth.setCredentials({ access_token: accessToken })
  return calendar({ version: `v3`, auth: oauth })
}

function buildEventBody(issue: Issue): calendar_v3.Schema$Event {
  const descriptionText = getIssueDescriptionText(issue.description)
  const appUrl = process.env.BETTER_AUTH_URL ?? ``
  const lines = [`Issue: ${issue.identifier}`]
  if (appUrl) lines.push(appUrl)
  if (descriptionText) {
    lines.push(``)
    lines.push(descriptionText)
  }

  return {
    summary: `[${issue.identifier}] ${issue.title}`,
    description: lines.join(`\n`),
    start: { date: issue.dueDate ?? undefined },
    end: { date: issue.dueDate ?? undefined },
  }
}

function shouldHaveEvent(issue: Issue): boolean {
  return (
    issue.dueDate !== null &&
    issue.status !== `done` &&
    issue.status !== `cancelled` &&
    issue.archivedAt === null
  )
}

async function persistSyncSuccess(
  issueId: string,
  patch: Partial<{
    googleCalendarEventId: string | null
  }>
) {
  await db
    .update(issues)
    .set({
      ...patch,
      googleCalendarLastSyncedAt: new Date(),
      googleCalendarLastSyncError: null,
    })
    .where(eq(issues.id, issueId))
}

async function persistSyncError(issueId: string, error: unknown) {
  const message = error instanceof Error ? error.message : String(error)
  try {
    await db
      .update(issues)
      .set({ googleCalendarLastSyncError: message.slice(0, 2000) })
      .where(eq(issues.id, issueId))
  } catch (writeError) {
    console.error(
      `google-calendar: failed to persist sync error`,
      writeError
    )
  }
}

async function reconcileIssueEvent(
  client: calendar_v3.Calendar,
  issue: Issue
): Promise<void> {
  const wantsEvent = shouldHaveEvent(issue)
  const existingEventId = issue.googleCalendarEventId

  if (wantsEvent && existingEventId) {
    try {
      await client.events.patch({
        calendarId: CALENDAR_ID,
        eventId: existingEventId,
        requestBody: buildEventBody(issue),
      })
      await persistSyncSuccess(issue.id, {})
      return
    } catch (error) {
      const status = (error as { code?: number }).code
      if (status === 404 || status === 410) {
        const created = await client.events.insert({
          calendarId: CALENDAR_ID,
          requestBody: buildEventBody(issue),
        })
        await persistSyncSuccess(issue.id, {
          googleCalendarEventId: created.data.id ?? null,
        })
        return
      }
      throw error
    }
  }

  if (wantsEvent && !existingEventId) {
    const created = await client.events.insert({
      calendarId: CALENDAR_ID,
      requestBody: buildEventBody(issue),
    })
    await persistSyncSuccess(issue.id, {
      googleCalendarEventId: created.data.id ?? null,
    })
    return
  }

  if (!wantsEvent && existingEventId) {
    try {
      await client.events.delete({
        calendarId: CALENDAR_ID,
        eventId: existingEventId,
      })
    } catch (error) {
      const status = (error as { code?: number }).code
      if (status !== 404 && status !== 410) throw error
    }
    await persistSyncSuccess(issue.id, { googleCalendarEventId: null })
    return
  }

  await persistSyncSuccess(issue.id, {})
}

export async function syncIssueToCalendar(
  userId: string,
  issue: Issue
): Promise<void> {
  try {
    const client = await getCalendarClient(userId)
    if (!client) return // user not connected — silent skip
    await reconcileIssueEvent(client, issue)
  } catch (error) {
    console.error(`google-calendar: sync failed for issue ${issue.id}`, error)
    await persistSyncError(issue.id, error)
  }
}

export async function deleteCalendarEventForIssue(
  userId: string,
  eventId: string
): Promise<void> {
  try {
    const client = await getCalendarClient(userId)
    if (!client) return
    await client.events.delete({
      calendarId: CALENDAR_ID,
      eventId,
    })
  } catch (error) {
    const status = (error as { code?: number }).code
    if (status === 404 || status === 410) return
    console.error(`google-calendar: delete failed for event ${eventId}`, error)
  }
}

export function fireAndForgetSync(userId: string, issue: Issue): void {
  syncIssueToCalendar(userId, issue).catch((error) => {
    console.error(`google-calendar: unhandled sync error`, error)
  })
}

export function fireAndForgetDelete(userId: string, eventId: string): void {
  deleteCalendarEventForIssue(userId, eventId).catch((error) => {
    console.error(`google-calendar: unhandled delete error`, error)
  })
}
