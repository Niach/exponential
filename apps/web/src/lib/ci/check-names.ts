// #EXP-2078: check names of our own CI next to GitHub Actions'. A GitHub
// repository with our CI on plans its `.github/workflows` too, and Actions
// runs the same workflows there: the same job would report under the same
// name from both, and a GitHub name wins a clash (the repair snapshot), so
// ours would vanish. Checks planned from a workflow on a GitHub repo
// therefore carry the prefix our own ci.json's jobs use by hand.

import { PIPELINE_NAME_MAX } from "@exp/ecode"

export const WORKFLOW_CHECK_PREFIX = `ecode `

/** A pipeline file read from a workflow, every job named `ecode <name>`
 *  (`needs` are job keys, so they still resolve). */
export function prefixWorkflowChecks<T>(file: T): T {
  if (!file || typeof file !== `object`) return file
  const jobs = (file as { jobs?: unknown }).jobs
  if (!jobs || typeof jobs !== `object`) return file
  const prefixed = Object.fromEntries(
    Object.entries(jobs as Record<string, unknown>).map(([key, job]) => {
      if (!job || typeof job !== `object`) return [key, job]
      const name = typeof (job as { name?: unknown }).name === `string` ? (job as { name: string }).name : key
      return [key, { ...job, name: Array.from(`${WORKFLOW_CHECK_PREFIX}${name}`).slice(0, PIPELINE_NAME_MAX).join(``) }]
    })
  )
  return { ...file, jobs: prefixed }
}

/** The repair snapshot: GitHub's checks plus ours (GitHub does not know
 *  them); a GitHub name wins a clash. */
export function mergeOwnChecks<C extends { name: string }>(github: readonly C[], own: readonly C[]): C[] {
  return [...github, ...own.filter((check) => !github.some((c) => c.name === check.name))]
}
