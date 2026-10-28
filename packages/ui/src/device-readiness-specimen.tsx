import deviceDoctor from "@exp/domain-contract/fixtures/device-doctor.json"
import { DeviceReadiness, deviceReadinessWarning, type DeviceReadinessDoctor } from "./device-readiness"

// FEED-160: the web twin of the natives' `sg_device-readiness` specimen
// (iOS StyleguideScreenshots, Android DeviceReadinessSpecimenScreen): the
// fixture's low-disk case as web draws it (always another device), the block
// in Device settings, then the composer's low-disk row. The app's
// `?dev=device-readiness` capture renders it.

interface DoctorCase {
  name: string
  doctor: DeviceReadinessDoctor
}
const CASES = deviceDoctor.cases as unknown as DoctorCase[]
const LOW_DISK = CASES[CASES.length - 1]
const noop = () => {}

function Caption({ children }: { children: string }) {
  return <p className="pt-2.5 text-[0.6875rem] text-muted-foreground/70">{children}</p>
}

export function DeviceReadinessSpecimen() {
  const warning = deviceReadinessWarning(LOW_DISK.doctor)
  return (
    <div data-testid="device-readiness-specimen" className="flex max-w-[23.5rem] flex-col gap-1.5">
      <Caption>{LOW_DISK.name}</Caption>
      <DeviceReadiness doctor={LOW_DISK.doctor} remote onAction={noop} />
      {warning ? (
        <>
          <Caption>Composer</Caption>
          <DeviceReadiness doctor={LOW_DISK.doctor} remote only={[warning]} onAction={noop} />
        </>
      ) : null}
    </div>
  )
}
