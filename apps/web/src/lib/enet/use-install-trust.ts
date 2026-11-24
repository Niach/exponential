// #EXP-1950 (fixtures/install-trust.json): this install's approval state for
// the UI: the rows Needs you draws (New install / Reset, Approve / Remove)
// and whether this install itself waits for approval (with its code).

import { useSyncExternalStore } from "react"
import { installCode, type TrustRow } from "@exp/domain-contract/install-trust"
import { currentKeyHolder, keyVersion, subscribeKeyChange } from "./current-key-holder"
import type { InstallTrustState } from "./install-trust"

export type InstallTrustView = {
  status: InstallTrustState
  rows: readonly TrustRow[]
  /** This install's own code (shown while it waits). */
  code: string | null
  approve: (row: TrustRow) => Promise<void>
  remove: (row: TrustRow) => Promise<void>
}

const NONE: InstallTrustView = {
  status: `unknown`,
  rows: [],
  code: null,
  approve: async () => {},
  remove: async () => {},
}

let cached: { version: number; view: InstallTrustView } | undefined

function snapshot(): InstallTrustView {
  const version = keyVersion()
  if (cached?.version === version) return cached.view
  const trust = currentKeyHolder()?.trust
  const view: InstallTrustView = trust
    ? {
        status: trust.status,
        rows: trust.rows,
        code: installCode(trust.endpointId),
        approve: (row) => trust.approve(row),
        remove: (row) => trust.remove(row),
      }
    : NONE
  cached = { version, view }
  return view
}

export function useInstallTrust(): InstallTrustView {
  return useSyncExternalStore(subscribeKeyChange, snapshot, () => NONE)
}
