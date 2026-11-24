// #EXP-1950 (fixtures/install-trust.json): this install's approval state for
// the UI: the rows Needs you draws (New install / Reset, Approve / Remove)
// and whether this install itself waits for approval (with its code).

import { useSyncExternalStore } from "react"
import fixture from "@exp/domain-contract/fixtures/install-trust.json" with { type: "json" }
import { installCode, type TrustRow } from "@exp/domain-contract/install-trust"
import { isProductionBuild } from "@/lib/production-build"
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
  const view = useSyncExternalStore(subscribeKeyChange, snapshot, () => NONE)
  return specimen() ?? view
}

/** DEV-ONLY (never in a production build): `?specimen=install-approval`
 *  draws the fixture's New install and Reset rows, `?specimen=install-waiting`
 *  this install waiting with its code (the catalogue's views). */
function specimen(): InstallTrustView | undefined {
  if (isProductionBuild || typeof window === `undefined`) return undefined
  const which = new URLSearchParams(window.location.search).get(`specimen`)
  if (which !== `install-approval` && which !== `install-waiting`) return undefined
  const keys = fixture.keys
  const row = (kind: TrustRow[`kind`], name: `N` | `T3`): TrustRow => ({
    kind,
    installId: `install-${name}`,
    endpointId: keys[name].endpointId,
    userId: keys[name].userId,
    label: keys[name].label,
    code: keys[name].code,
  })
  return which === `install-approval`
    ? { ...NONE, status: `trusted`, rows: [row(`newInstall`, `N`), row(`reset`, `T3`)] }
    : { ...NONE, status: `waiting`, code: keys.N.code }
}
