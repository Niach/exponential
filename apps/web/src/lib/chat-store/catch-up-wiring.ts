// #EDE-115: the catch-up loop wired to this tab (loaded on demand by the tab
// that wins the eNet node's Web Lock, use-web-install.ts): the node, the
// store's worker, the synced sessions, the team registries, power and
// idleness.

import { sessionCollection } from "@/lib/collections"
import type { WebInstall } from "@/lib/enet/web-install"
import { trpc } from "@/lib/trpc-client"
import { startCatchUp, type CatchUpInstall, type CatchUpRow } from "./catch-up"
import { chatStoreCall, lastActiveAt } from "./client"

type Battery = { charging: boolean }

const quiet = { context: { skipErrorToast: true } }

export function wireCatchUp(install: WebInstall, userId: string): () => void {
  let battery: Battery | undefined
  const nav = navigator as Navigator & { getBattery?: () => Promise<Battery> }
  void nav
    .getBattery?.()
    .then((found) => (battery = found))
    .catch(() => {})
  return startCatchUp({
    userId,
    node: install,
    store: chatStoreCall,
    sessions: () => [...sessionCollection.values()] as unknown as CatchUpRow[],
    onSessionsChange: (listener) => {
      const sub = sessionCollection.subscribeChanges(() => listener())
      return () => sub.unsubscribe()
    },
    installs: (teamId) => trpc.installs.list.query({ teamId }, quiet) as Promise<CatchUpInstall[]>,
    power: () => (battery ? (battery.charging ? `ac` : `battery`) : `unknown`),
    idleMs: () => Date.now() - lastActiveAt(),
  })
}
