/**
 * One-off (SLOP-3): dissolve every live GitHub PR stack Exponential created,
 * BEFORE the deploy that drops `issues.pr_stack_number`. A stack member can
 * only merge through merge-async and its base is managed by the stack; once
 * the column is gone nothing knows the stack number any more.
 *
 * Usage (from apps/web): bun scripts/unstack-live-stacks.ts [--dry-run]
 */
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  githubApiHeaders,
  githubAppConfigured,
  resolveRepoInstallationToken,
} from "@/lib/integrations/github-app"

const STACK_API_VERSION = `2026-03-10`
const PR_URL = /^https:\/\/github\.com\/([^/]+\/[^/]+)\/pull\/\d+/

async function unstack(repo: string, stackNumber: number, token: string) {
  const res = await fetch(
    `https://api.github.com/repos/${repo}/stacks/${stackNumber}/unstack`,
    {
      method: `POST`,
      headers: {
        ...githubApiHeaders(token),
        "x-github-api-version": STACK_API_VERSION,
      },
    }
  )
  if (res.status === 404 || res.status === 422) return `gone (${res.status})`
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`)
  return `unstacked (${res.status})`
}

async function main() {
  const dryRun = process.argv.includes(`--dry-run`)
  if (!githubAppConfigured()) {
    console.error(`[unstack] GitHub App is not configured. Nothing to do.`)
    process.exit(1)
  }
  const result = await db.execute(sql`
    SELECT DISTINCT pr_url, pr_stack_number
    FROM issues
    WHERE pr_stack_number IS NOT NULL AND pr_state = 'open' AND pr_url IS NOT NULL
  `)
  const rows = result.rows as Array<{ pr_url: string; pr_stack_number: number }>
  const stacks = new Map<string, { repo: string; stackNumber: number }>()
  for (const row of rows) {
    const repo = PR_URL.exec(row.pr_url)?.[1]
    if (!repo) continue
    stacks.set(`${repo}#${row.pr_stack_number}`, {
      repo,
      stackNumber: Number(row.pr_stack_number),
    })
  }
  console.log(`[unstack] ${stacks.size} live stack(s)`)
  let failed = 0
  for (const { repo, stackNumber } of stacks.values()) {
    if (dryRun) {
      console.log(`[unstack] would unstack ${repo} stack ${stackNumber}`)
      continue
    }
    try {
      const token = await resolveRepoInstallationToken(repo)
      if (!token) throw new Error(`no installation token`)
      console.log(`[unstack] ${repo} stack ${stackNumber}: ${await unstack(repo, stackNumber, token)}`)
    } catch (err) {
      failed++
      console.error(`[unstack] ${repo} stack ${stackNumber} failed:`, err)
    }
  }
  process.exit(failed ? 1 : 0)
}

void main()
