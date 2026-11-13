// #EXP-1616: the names eCode's own git accepts (shared by the router and
// the repo operations).
import { z } from "zod"

/** A repo name: one `exp://<team>/<name>` path segment (remote.json). */
export const ecodeRepoName = z
  .string()
  .min(1)
  .max(100)
  .regex(/^[A-Za-z0-9._-]+$/, `Use letters, digits, '.', '_' or '-'`)
  .refine((name) => !name.startsWith(`.`) && !name.endsWith(`.git`), `Not a valid repository name`)

export const branchName = z
  .string()
  .min(1)
  .max(200)
  .refine((name) => /^[A-Za-z0-9._/-]+$/.test(name) && !name.includes(`..`) && !name.startsWith(`/`) && !name.endsWith(`/`), `Not a valid branch name`)
