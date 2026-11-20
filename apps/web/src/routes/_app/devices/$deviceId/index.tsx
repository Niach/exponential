import { createFileRoute } from "@tanstack/react-router"
import { DeviceDetailPage } from "@/components/devices/device-page"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027 (sidebar-tree.json DEVICES): a device row's DETAIL page: its
// accounts with their limits and the sessions running on it across teams.
export const Route = createFileRoute(`/_app/devices/$deviceId/`)({
  head: () => ({ meta: [{ title: pageTitle(`Device`) }] }),
  component: DeviceRoute,
})

function DeviceRoute() {
  const { deviceId } = Route.useParams()
  const { user } = Route.useRouteContext()
  return <DeviceDetailPage key={deviceId} deviceId={deviceId} userId={user.id} />
}
