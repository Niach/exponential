import { createFileRoute } from "@tanstack/react-router"
import { DEVICES_LIST_COPY } from "@exp/domain-contract/devices-list"
import { DeviceSettingsPage } from "@/components/devices/device-page"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027 (sidebar-tree.json DEVICES): a device row's gear opens this PAGE,
// the old Device settings dialog's body (`DeviceSettingsBody`).
export const Route = createFileRoute(`/_app/devices/$deviceId/settings`)({
  head: () => ({ meta: [{ title: pageTitle(DEVICES_LIST_COPY.settings) }] }),
  component: DeviceSettingsRoute,
})

function DeviceSettingsRoute() {
  const { deviceId } = Route.useParams()
  const { user } = Route.useRouteContext()
  return <DeviceSettingsPage key={deviceId} deviceId={deviceId} userId={user.id} />
}
