import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"
import { emailEnabled } from "@/lib/email-enabled"
import { runGithubIdentityBackfill } from "@/lib/integrations/github-identity-backfill"
// Vite's ?raw suffix inlines file contents as a string at build time. We
// do this so the server bundle ships the SQL alongside the JS, no fs reads
// required at runtime (which Vite also can't tree-shake for browser builds).
import triggersSql from "@/db/out/custom/0001_triggers.sql?raw"

function parseAdminEmails(): string[] {
  const raw = process.env.INITIAL_ADMIN_EMAILS
  if (!raw) return []
  return raw
    .split(`,`)
    .map((email) => email.trim().toLowerCase())
    .filter(Boolean)
}

async function promoteInitialAdmins() {
  const emails = parseAdminEmails()
  if (emails.length === 0) return

  await db
    .update(users)
    .set({ isAdmin: true, updatedAt: new Date() })
    .where(
      and(
        sql`lower(${users.email}) IN (${sql.join(
          emails.map((email) => sql`${email}`),
          sql`, `
        )})`,
        // With open sign-up anyone can create a row for an admin email, so
        // promotion must wait for proven mailbox ownership. Skip the gate when
        // email flows are off (no way to ever verify on such instances).
        emailEnabled ? eq(users.emailVerified, true) : undefined
      )
    )
}

// Drizzle migrations don't run our hand-written triggers + partial unique
// index. Apply them on every boot — every statement is idempotent
// (CREATE OR REPLACE / CREATE … IF NOT EXISTS), so "already exists" is not a
// possible failure here: any error is a real one (connection blip, pool
// exhaustion, missing TRIGGER privilege) and must propagate. Several triggers
// exist ONLY via this path — swallowing a failure would leave the instance
// running trigger-less until the next restart (REV-18).
async function applyCustomSql() {
  for (const [name, content] of [[`0001_triggers.sql`, triggersSql]] as const) {
    if (!content) continue
    try {
      await db.execute(sql.raw(content))
    } catch (err) {
      throw new Error(`applying ${name} failed`, { cause: err })
    }
  }
}

// Opt-IN cloud marker (EXP-364; replaced the opt-out SELF_HOSTED flag): only
// app.exponential.at / next.exponential.at set CLOUD_INSTANCE=true. Every
// other deployment — including a hand-rolled one that sets nothing — runs in
// self-hosted mode: billing off, plan limits unlocked, no dogfood widget.
export function isCloudInstance(): boolean {
  return (process.env.CLOUD_INSTANCE ?? ``).toLowerCase() === `true`
}

let bootstrapPromise: Promise<void> | null = null

// One attempt of the boot-time work, throwing on failure. Exported for tests;
// bootstrapCloud() below is the only production caller.
export async function runBootstrapPass(): Promise<void> {
  await applyCustomSql()
  await promoteInitialAdmins()
}

const RETRY_BASE_DELAY_MS = 5 * 1000
const RETRY_MAX_DELAY_MS = 5 * 60 * 1000

// Boot-time bootstrap. EXP-364 removed the dogfood machinery entirely (the
// feedback team, its comp/helpdesk forcing, and the widget-config seeding are
// ordinary hand-managed rows on the cloud now — nothing recreates or heals
// them); what remains is instance-agnostic.
//
// The sole caller (server-bun.ts) invokes this exactly once, fire-and-forget,
// so a failed pass would never be re-applied until the next process restart.
// Every pass is idempotent, so instead of surfacing failure we retry with
// capped backoff until the pass applies cleanly — the returned promise
// resolves only on success and never rejects (REV-18). A permanently broken
// setup (e.g. a DB role without TRIGGER privilege) keeps logging at error
// level every few minutes instead of degrading silently.
export function bootstrapCloud(): Promise<void> {
  if (bootstrapPromise) return bootstrapPromise
  bootstrapPromise = (async () => {
    for (let attempt = 1; ; attempt++) {
      try {
        await runBootstrapPass()
        // EXP-617: deliberately OUTSIDE the retrying pass and not awaited.
        // It talks to GitHub, so it must never be able to fail a boot or
        // trigger the backoff loop above — and nothing downstream waits on
        // it (an unmapped user just keeps today's attribution until the
        // next restart or their next GitHub connect).
        void runGithubIdentityBackfill()
        return
      } catch (err) {
        const delayMs = Math.min(
          RETRY_BASE_DELAY_MS * 2 ** (attempt - 1),
          RETRY_MAX_DELAY_MS
        )
        console.error(
          `[bootstrap-cloud] attempt ${attempt} failed (retrying in ${Math.round(delayMs / 1000)}s):`,
          err
        )
        await new Promise((resolve) => setTimeout(resolve, delayMs))
      }
    }
  })()
  return bootstrapPromise
}

// Promote a single user if their email matches the admin list. Used by Better
// Auth's user.create.after hook (so first-sign-in promotion doesn't need to
// wait for a server restart) and again from afterEmailVerification.
//
// When email flows are enabled, promotion requires a verified email: sign-up
// is open on the cloud and does not prove mailbox ownership, so an attacker
// could otherwise register an INITIAL_ADMIN_EMAILS address before its real
// owner and walk away with a global-admin session.
export async function maybePromoteNewUser(
  userId: string,
  email: string,
  emailVerified: boolean
) {
  if (emailEnabled && !emailVerified) return
  const emails = parseAdminEmails()
  if (emails.length === 0) return
  if (!emails.includes(email.toLowerCase())) return
  await db
    .update(users)
    .set({ isAdmin: true, updatedAt: new Date() })
    .where(eq(users.id, userId))
}
