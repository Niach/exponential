// Last-used Start-coding dialog options (EXP-149). Per-device via guarded
// localStorage (`safeLocalStorage` — degrades to "no persistence").
//
// Values are validated against the domain contract on read, so a stale entry
// from an older build can never seed the dialog with a value the server's
// zod enums would reject.

import { contract } from "@exp/domain-contract"
import { safeLocalStorage } from "@/lib/local-storage"

export interface CodingLaunchPrefs {
  model: string
  /** `""` = "CLI default" (omit --effort) — a valid stored value. */
  effort: string
  ultracode: boolean
  planMode: boolean
}

export const DEFAULT_CODING_LAUNCH_PREFS: CodingLaunchPrefs = {
  model: contract.codingModel.values[0],
  effort: ``,
  ultracode: false,
  // Plan mode defaults OFF for remote starts: the session runs on an
  // unattended desktop, and only web steer can approve a plan remotely.
  planMode: false,
}

const STORAGE_KEY = `exp.codingLaunchOptions`

export function readCodingLaunchPrefs(): CodingLaunchPrefs {
  const store = safeLocalStorage()
  if (!store) return DEFAULT_CODING_LAUNCH_PREFS
  try {
    const raw = store.getItem(STORAGE_KEY)
    if (!raw) return DEFAULT_CODING_LAUNCH_PREFS
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== `object` || parsed === null) {
      return DEFAULT_CODING_LAUNCH_PREFS
    }
    const record = parsed as Record<string, unknown>
    return {
      model:
        typeof record.model === `string` &&
        contract.codingModel.values.includes(record.model)
          ? record.model
          : DEFAULT_CODING_LAUNCH_PREFS.model,
      effort:
        typeof record.effort === `string` &&
        (record.effort === `` ||
          contract.codingEffort.values.includes(record.effort))
          ? record.effort
          : DEFAULT_CODING_LAUNCH_PREFS.effort,
      ultracode:
        typeof record.ultracode === `boolean`
          ? record.ultracode
          : DEFAULT_CODING_LAUNCH_PREFS.ultracode,
      planMode:
        typeof record.planMode === `boolean`
          ? record.planMode
          : DEFAULT_CODING_LAUNCH_PREFS.planMode,
    }
  } catch {
    return DEFAULT_CODING_LAUNCH_PREFS
  }
}

export function rememberCodingLaunchPrefs(prefs: CodingLaunchPrefs): void {
  const store = safeLocalStorage()
  if (!store) return
  try {
    store.setItem(STORAGE_KEY, JSON.stringify(prefs))
  } catch {
    // Quota/privacy failures just mean no persistence — never block a start.
  }
}
