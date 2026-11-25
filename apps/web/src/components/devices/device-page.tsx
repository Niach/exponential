import { useNavigate } from "@tanstack/react-router"
import { Button, conceptIcon, getDeviceIcon, HeaderAction, Separator, SkeletonRows } from "@exp/ui"
import { SIDEBAR_TREE_COPY } from "@exp/domain-contract/sidebar-tree"
import { DEVICES_LIST_COPY } from "@exp/domain-contract/devices-list"
import { deviceIsMine, deviceIsOnline, deviceUpdateAvailable, type SteerDevice } from "@/lib/steer-devices"
import { useNow } from "@/hooks/use-now"
import { usePageTitle, pageTitle } from "@/lib/page-title"
import { DeviceStatusLine } from "@/components/device-setup"
import { DeviceSettingsBody } from "@/components/device-settings-dialog"
import { Empty } from "@/components/empty"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { DeviceAccountsSection } from "@/components/devices/device-accounts-section"
import { DeviceRunningSection } from "@/components/devices/device-running-section"
import { useDevicePage } from "@/components/devices/use-device-page"

// #EXP-2027 (sidebar-tree.json DEVICES): a device row opens its DETAIL page
// (the header, the logins with their limits, the sessions running on it
// across the viewer's teams); its gear opens the SETTINGS page (the Device
// settings dialog's body as a page). Team-less pages of web md+.

const SettingsIcon = conceptIcon(`nav-settings`)
const BackIcon = conceptIcon(`ui-back`)

const PAGE_FRAME = `mx-auto w-full max-w-4xl space-y-6 p-4 sm:p-6 ${PAGE_END_CLEARANCE}`

/** The device's label (the id when it never named itself). */
const deviceName = (device: SteerDevice) => device.deviceLabel || device.deviceId

function DeviceHeading({
  device,
  latestVersions,
  leading,
  trailing,
}: {
  device: SteerDevice
  latestVersions: { desktop: string | null; cli: string | null } | null
  leading?: React.ReactNode
  trailing?: React.ReactNode
}) {
  const DeviceIcon = getDeviceIcon({ icon: device.icon, kind: device.kind })
  const latest = (device.kind === `server` ? latestVersions?.cli : latestVersions?.desktop) ?? null
  const outdated = deviceIsMine(device) && deviceUpdateAvailable(device.version, latest)
  return (
    <div className="flex min-w-0 items-center gap-3">
      {leading}
      <DeviceIcon aria-hidden className="size-6 shrink-0 text-muted-foreground" />
      <h1 className="min-w-0 truncate text-2xl font-bold">{deviceName(device)}</h1>
      {device.version && (
        <span
          className={`shrink-0 text-xs ${outdated ? `text-yellow-400` : `text-muted-foreground`}`}
          title={outdated && latest ? `Update available: ${latest}` : undefined}
        >
          v{device.version}
        </span>
      )}
      <div className="shrink-0">
        <DeviceStatusLine online={deviceIsOnline(device)} lastSeenAt={device.lastSeenAt} />
      </div>
      {trailing && <div className="ml-auto flex shrink-0 items-center gap-2">{trailing}</div>}
    </div>
  )
}

/** `/devices/$deviceId`: the device's detail page. */
export function DeviceDetailPage({ deviceId, userId }: { deviceId: string; userId: string }) {
  const navigate = useNavigate()
  const page = useDevicePage(deviceId, userId)
  const now = useNow(30_000)
  const device = page.device
  usePageTitle(device ? pageTitle(deviceName(device)) : undefined)
  if (!device) return <DeviceMissing ready={page.ready} />
  return (
    <div className={PAGE_FRAME} data-testid="device-page">
      <DeviceHeading
        device={device}
        latestVersions={page.latestVersions}
        trailing={
          page.mine && device.registered ? (
            <HeaderAction
              aria-label={SIDEBAR_TREE_COPY.deviceSettings}
              title={SIDEBAR_TREE_COPY.deviceSettings}
              onClick={() => void navigate({ to: `/devices/$deviceId/settings`, params: { deviceId } })}
              data-testid="device-page-settings"
            >
              <SettingsIcon />
            </HeaderAction>
          ) : undefined
        }
      />
      {/* A teammate's shared server shows its logins read-only, as the old
          Team devices card did. */}
      <DeviceAccountsSection device={device} now={now} latestVersions={page.latestVersions} />
      <DeviceRunningSection deviceId={device.deviceId} teams={page.teams} />
    </div>
  )
}

/** `/devices/$deviceId/settings`: the Device settings dialog's body as a
 *  page, the viewer's own machines only. */
export function DeviceSettingsPage({ deviceId, userId }: { deviceId: string; userId: string }) {
  const navigate = useNavigate()
  const page = useDevicePage(deviceId, userId)
  const device = page.device
  usePageTitle(device ? pageTitle(DEVICES_LIST_COPY.settings, deviceName(device)) : undefined)
  if (!device || !page.mine) return <DeviceMissing ready={page.ready} />
  const back = () => void navigate({ to: `/devices/$deviceId`, params: { deviceId } })
  return (
    <div className={PAGE_FRAME} data-testid="device-settings-page">
      <DeviceHeading
        device={device}
        latestVersions={page.latestVersions}
        leading={
          <Button
            variant="ghost"
            size="icon"
            aria-label={deviceName(device)}
            className="-ml-2 size-9 shrink-0 text-muted-foreground hover:text-foreground"
            onClick={back}
            data-testid="device-settings-back"
          >
            <BackIcon className="size-5" />
          </Button>
        }
      />
      <Separator />
      <DeviceSettingsBody
        device={device}
        latestVersions={page.latestVersions}
        onChanged={page.refresh}
        // The device is gone: so are its pages.
        onRemoved={() => void navigate({ to: `/home`, replace: true })}
        className="flex flex-col gap-2"
      />
    </div>
  )
}

/** Loading = the header + rows skeleton; a device the viewer cannot see =
 *  the devices empty state. */
function DeviceMissing({ ready }: { ready: boolean }) {
  return (
    <div className={PAGE_FRAME}>
      {ready ? <Empty id="devices" size="page" /> : <SkeletonRows rowHeight={52} />}
    </div>
  )
}
