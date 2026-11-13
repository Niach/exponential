// #EXP-1616: the optional mirror of an `ecode` repo (`provider_config.
// mirrorUrl`), pushed by a team device after each landing and each branch
// deletion (`ecode_job` op
// `mirror`: every branch and tag, forced and pruned, so the mirror equals
// the bucket: a branch deleted here, a merged head included, goes there too). A GitHub
// mirror the team's GitHub App covers gets a fresh installation token in the
// job (short-lived, read only by the device that runs it); any other https
// remote is pushed with the device's own git credentials.
import { parseRemote } from "@exp/ecode"
import type { EcodeMaintenanceJob } from "@/lib/ecode/jobs"

/** A mirror URL the settings accept: an https remote (remote.json `parse`). */
export function validMirrorUrl(url: string): boolean {
  return parseRemote(url)?.scheme === `https`
}

export async function mirrorJob(url: string): Promise<EcodeMaintenanceJob> {
  const remote = parseRemote(url)
  if (remote?.scheme === `https` && remote.host === `github.com`) {
    const { resolveRepoInstallationTokenInfo } = await import(`@/lib/integrations/github-app`)
    const resolved = await resolveRepoInstallationTokenInfo(remote.path).catch(() => null)
    if (resolved) return { op: `mirror`, url: `https://github.com/${remote.path}.git`, username: `x-access-token`, token: resolved.token }
  }
  return { op: `mirror`, url }
}
