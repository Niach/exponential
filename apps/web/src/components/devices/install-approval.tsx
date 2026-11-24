import { useLiveQuery } from "@tanstack/react-db"
import type { NestedNode } from "@exponential_app/eui"
import type { SurfaceActionEvent } from "@exponential_app/eui-react"
import { INSTALL_TRUST_COPY, type TrustRow } from "@exp/domain-contract/install-trust"
import { toast } from "@exp/ui"
import { CiSurface } from "@/components/ci/ci-surface"
import { userCollection } from "@/lib/collections"
import { useTreeKeys } from "@/lib/enet/use-key-holder"
import { useInstallTrust, type InstallTrustView } from "@/lib/enet/use-install-trust"

// #EXP-1950 (fixtures/install-trust.json, decisions a1 + b1): the approval of
// a new install and a team-mate's reset, drawn ONLY from core genUI parts
// (Section, Row, Badge, Button, EmptyState) so the four platforms paint the
// same tree. Rows: Needs you on every form factor, the phone's Devices too.
// The code is derived from the row's endpoint id on THIS install, never read
// from the server. A failed action = the toast (feedback-states.json).

function pendingRow(row: TrustRow, title: string): NestedNode {
  const copy = INSTALL_TRUST_COPY
  return {
    id: `install-${row.installId}`,
    component: `Row`,
    props: { title, subtitle: row.code },
    slots: {
      trailing: {
        id: `install-${row.installId}-actions`,
        component: `Stack`,
        props: { direction: `horizontal`, gap: `sm` },
        children: [
          { id: `install-${row.installId}-badge`, component: `Badge`, props: { text: copy[row.kind], variant: `secondary` } },
          {
            id: `install-${row.installId}-approve`,
            component: `Button`,
            props: { label: copy.approve, size: `sm` },
            on: { press: { event: { name: `approve`, context: { id: row.installId } } } },
          },
          {
            id: `install-${row.installId}-remove`,
            component: `Button`,
            props: { label: copy.remove, size: `sm`, variant: `ghost` },
            on: { press: { event: { name: `remove`, context: { id: row.installId } } } },
          },
        ],
      },
    },
  }
}

/** A row's title: the install's label; a team-mate's leads with their name. */
function useRowTitles(): (row: TrustRow) => string {
  const { data: users } = useLiveQuery((query) => query.from({ users: userCollection }), [])
  const names = new Map(((users ?? []) as Array<{ id: string; name?: string | null }>).map((user) => [user.id, user.name ?? ``]))
  return (row) => {
    const label = row.label ?? ``
    if (row.kind !== `reset`) return label || row.code
    const name = names.get(row.userId)
    return [name, label].filter(Boolean).join(` · `) || row.code
  }
}

/** The band of installs waiting for this install's approval (nothing when none). */
export function PendingInstalls({ trust }: { trust?: InstallTrustView }) {
  const live = useInstallTrust()
  const view = trust ?? live
  const titleOf = useRowTitles()
  if (view.rows.length === 0) return null
  const tree: NestedNode = {
    id: `pending-installs`,
    component: `Section`,
    props: {},
    children: view.rows.map((row) => pendingRow(row, titleOf(row))),
  }
  const onAction = (event: SurfaceActionEvent) => {
    const id = (event.payload as { id?: string } | undefined)?.id
    const row = view.rows.find((candidate) => candidate.installId === id)
    if (!row) return
    const run = event.name === `approve` ? view.approve(row) : event.name === `remove` ? view.remove(row) : undefined
    void run?.catch((error: unknown) => toast.error(error instanceof Error ? error.message : String(error)))
  }
  return (
    <div data-testid="pending-installs" className="w-full min-w-0">
      <CiSurface surfaceId="pending-installs" tree={tree} onAction={onAction} />
    </div>
  )
}

/** This install, not approved yet: THE page EmptyState with its own code. */
export function InstallWaiting({ code }: { code: string }) {
  const tree: NestedNode = {
    id: `install-waiting`,
    component: `EmptyState`,
    props: { icon: `ui-private`, title: INSTALL_TRUST_COPY.waiting, description: code },
  }
  return (
    <div data-testid="install-waiting" className="mx-auto w-full max-w-md pt-[20vh]">
      <CiSurface surfaceId="install-waiting" tree={tree} />
    </div>
  )
}

/** Where an encrypted chat would open on an install that waits for approval
 *  and holds no key of it: the waiting state instead of locked rows. */
export function InstallWaitingGate({ rootId, sealed }: { rootId: string; sealed: boolean }) {
  const trust = useInstallTrust()
  const keys = useTreeKeys(rootId)
  if (!sealed || trust.status !== `waiting` || keys.size > 0 || !trust.code) return null
  return <InstallWaiting code={trust.code} />
}
