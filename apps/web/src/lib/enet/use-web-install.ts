// EXP-1457 step 1: runs this browser install's eNet node while a user is
// signed in (mounted by the _authenticated layout). One node per browser
// profile: tabs share the identity, and two nodes with one endpoint id would
// fight over the relay, so only the tab holding the Web Lock runs it; when
// that tab closes the next one takes over.

import { useEffect, useMemo, useRef } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { teamMemberCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { loadEnet } from "./enet-loader"
import { WebInstall, type InstallsApi } from "./web-install"

const LOCK = `exp-enet-node`

const api: InstallsApi = {
  register: (input) => trpc.installs.register.mutate(input, { context: { skipErrorToast: true } }),
  list: (teamId) => trpc.installs.list.query({ teamId }),
  revoke: (installId) =>
    trpc.installs.revoke.mutate({ installId }, { context: { skipErrorToast: true } }),
}

let current: WebInstall | undefined

/** Explicit sign-out: stop the node, revoke this install, forget its identity. */
export async function signOutWebInstall(): Promise<void> {
  const install = current
  current = undefined
  await install?.stop({ revoke: true })
}

export function useWebInstall(userId: string | undefined) {
  const { data: memberships } = useLiveQuery(
    (q) =>
      userId
        ? q.from({ m: teamMemberCollection }).where(({ m }) => eq(m.userId, userId))
        : undefined,
    [userId]
  )
  const teamKey = useMemo(
    () => [...new Set((memberships ?? []).map((m) => m.teamId))].sort().join(`,`),
    [memberships]
  )

  const installRef = useRef<WebInstall | undefined>(undefined)
  const teamKeyRef = useRef(teamKey)
  teamKeyRef.current = teamKey

  useEffect(() => {
    if (!userId || typeof navigator === `undefined` || !navigator.locks) return
    const abort = new AbortController()
    const install = new WebInstall(userId, { loadEnet, api })
    installRef.current = install
    void install.setTeams(splitTeams(teamKeyRef.current))
    navigator.locks
      .request(LOCK, { signal: abort.signal }, async () => {
        current = install
        await install.start()
        // Hold the lock until this effect ends.
        await new Promise<void>((resolve) => abort.signal.addEventListener(`abort`, () => resolve()))
      })
      .catch((err: unknown) => {
        if (abort.signal.aborted) return
        console.warn(`eNet node did not start`, err)
      })
    return () => {
      abort.abort()
      if (current === install) current = undefined
      if (installRef.current === install) installRef.current = undefined
      void install.stop()
    }
  }, [userId])

  useEffect(() => {
    installRef.current?.setTeams(splitTeams(teamKey)).catch(() => {})
  }, [teamKey])
}

function splitTeams(key: string): string[] {
  return key ? key.split(`,`) : []
}
