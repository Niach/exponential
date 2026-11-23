import { useState } from "react"
import type { NestedNode } from "@exponential_app/eui"
import type { SurfaceActionEvent } from "@exponential_app/eui-react"
import { CiSurface } from "@/components/ci/ci-surface"

// #EXP-1950 DRAFT (pictures for Danny, nothing ported, no server): the
// confirmation of a new install (decision a1) and the team-mate's reset
// (decision b1), drawn ONLY from core genUI parts (Section, Row, Badge,
// Button, EmptyState) so the four platforms paint the same tree. The
// pending list is a LOCAL STUB. Every entry point (`?draft=install-approval`
// on the phone's Devices; `?dev=install-approval` and `?dev=install-waiting`
// on Home, md+ having no Devices page) is never parsed in a production build
// (`isProductionBuild`).
//
// The short code: 10 Crockford base32 characters (50 bits, no I L O U) in
// two groups of five, made from the install's ed25519 public key (its
// endpoint id): SHA-256("exp/enet/install-code/1" ‖ 0x00 ‖ endpointId[32]),
// the first 50 bits. Both ends derive it from the key itself; the trusted
// side never reads a code from the server.

export const INSTALL_APPROVAL_COPY = {
  newInstall: `New install`,
  reset: `Reset`,
  approve: `Approve`,
  remove: `Remove`,
  waiting: `Waiting for approval`,
} as const

type PendingInstall = {
  id: string
  /** The install's own label; a team-mate's row leads with their name. */
  title: string
  code: string
  kind: `newInstall` | `reset`
}

// The stub: one own new install, one team-mate's reset (every install of
// theirs lost). Seed names (Mira Chen is the screenshot seed's team-mate).
const PENDING_STUB: PendingInstall[] = [
  { id: `own-pixel`, title: `Pixel 9`, code: `K7QF3 M9XW2`, kind: `newInstall` },
  { id: `mira-iphone`, title: `Mira Chen · iPhone 17`, code: `4TZ8H R1CVN`, kind: `reset` },
]

const NEW_INSTALL_CODE = PENDING_STUB[0]!.code

function pendingRow(row: PendingInstall): NestedNode {
  const copy = INSTALL_APPROVAL_COPY
  return {
    id: `install-${row.id}`,
    component: `Row`,
    props: { title: row.title, subtitle: row.code },
    slots: {
      trailing: {
        id: `install-${row.id}-actions`,
        component: `Stack`,
        props: { direction: `horizontal`, gap: `sm` },
        children: [
          { id: `install-${row.id}-badge`, component: `Badge`, props: { text: copy[row.kind], variant: `secondary` } },
          {
            id: `install-${row.id}-approve`,
            component: `Button`,
            props: { label: copy.approve, size: `sm` },
            on: { press: { event: { name: `approve`, context: { id: row.id } } } },
          },
          {
            id: `install-${row.id}-remove`,
            component: `Button`,
            props: { label: copy.remove, size: `sm`, variant: `ghost` },
            on: { press: { event: { name: `remove`, context: { id: row.id } } } },
          },
        ],
      },
    },
  }
}

/** (i) + (iii): the bare band of pending rows over My devices. */
export function PendingInstallsDraft() {
  const [rows, setRows] = useState(PENDING_STUB)
  if (rows.length === 0) return null
  const tree: NestedNode = {
    id: `pending-installs`,
    component: `Section`,
    props: {},
    children: rows.map(pendingRow),
  }
  const onAction = (event: SurfaceActionEvent) => {
    const id = (event.payload as { id?: string } | undefined)?.id
    if (id) setRows((current) => current.filter((row) => row.id !== id))
  }
  return (
    <div data-testid="pending-installs-draft" className="mb-6 w-full min-w-0">
      <CiSurface surfaceId="pending-installs-draft" tree={tree} onAction={onAction} />
    </div>
  )
}

/** (ii): the new install itself, where an encrypted chat would open. */
export function InstallWaitingDraft() {
  const tree: NestedNode = {
    id: `install-waiting`,
    component: `EmptyState`,
    props: { icon: `ui-private`, title: INSTALL_APPROVAL_COPY.waiting, description: NEW_INSTALL_CODE },
  }
  return (
    <div data-testid="install-waiting-draft" className="mx-auto w-full max-w-md pt-[20vh]">
      <CiSurface surfaceId="install-waiting-draft" tree={tree} />
    </div>
  )
}
