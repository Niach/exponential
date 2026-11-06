import type { TrainCheck } from "@exp/domain-contract/merge-train"

/** EXP-1709: a GitHub commit status state → the provider-neutral status +
 *  conclusion (a required context may be a status, not a check run). */
export function checkFromGithubStatus(state: string | null | undefined): Pick<TrainCheck, `status` | `conclusion`> {
  if (state === `success`) return { status: `completed`, conclusion: `success` }
  if (state === `failure` || state === `error`) return { status: `completed`, conclusion: `failure` }
  return { status: `in_progress`, conclusion: null }
}
