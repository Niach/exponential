import { useState } from "react"
import deviceDoctor from "@exp/domain-contract/fixtures/device-doctor.json"
import { DeviceReadiness, type DeviceReadinessDoctor } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1196/1218/1219: THE device readiness block. The REAL `@exp/ui`
// `DeviceReadiness` over the three cases of the contract fixture
// `device-doctor.json`, each on the device itself (every action offered) and
// from another device (remote: only Update / Sign in).

interface DoctorCase {
  name: string
  doctor: DeviceReadinessDoctor
}
const CASES = deviceDoctor.cases as unknown as DoctorCase[]
const noop = () => {}

function Specimen({ doctor, remote }: { doctor: DeviceReadinessDoctor; remote: boolean }) {
  const initial = doctor.items.find((item) => item.key === `computer_use`)?.state !== `off`
  const [computerUse, setComputerUse] = useState(initial)
  return (
    <DeviceReadiness
      doctor={doctor}
      remote={remote}
      onAction={noop}
      computerUse={{ checked: computerUse, onCheckedChange: setComputerUse }}
    />
  )
}

export const entry: StyleguideEntry = {
  id: `device-readiness`,
  section: `special`,
  owner: `EXP-1196`,
  title: `Device readiness`,
  blurb: `One block ×5 from the device's doctor report (device-doctor.json): a band per group (Required, Coding agents, Computer use; the tag trailing), one flat row per item with its state glyph, label, the device's detail and at most one pill. Computer use is the switch row; its permission rows indent under it and hide while it is off. On the device every action is offered, from another device only Update and Sign in; the first action/error row's pill is primary. No subtitles, no footers.`,
  status: {
    web: {
      state: `ok`,
      symbol: `DeviceReadiness`,
      file: `packages/ui/src/device-readiness.tsx`,
    },
    desktop: {
      state: `leftover`,
      symbol: `coding::device_doctor`,
      file: `apps/desktop/crates/coding/src/device_doctor.rs`,
      note: `The report is built here; the IDE's block lands with EXP-1196.`,
    },
    ios: {
      state: `ok`,
      symbol: `DeviceReadinessView`,
      file: `apps/ios/Exponential/UI/Components/DeviceReadinessView.swift`,
    },
    android: {
      state: `ok`,
      symbol: `DeviceReadinessBlock`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/DeviceReadiness.kt`,
    },
  },
  island: () => (
    <div className="flex flex-col gap-6">
      {CASES.map((fixtureCase) => (
        <div key={fixtureCase.name} className="flex flex-col gap-2">
          <div className="text-xs text-muted-foreground">{fixtureCase.name}</div>
          <div className="flex flex-wrap gap-4">
            {([false, true] as const).map((remote) => (
              <div key={String(remote)} className="flex w-full max-w-[23.5rem] flex-col gap-1.5">
                <div className="text-[0.6875rem] uppercase tracking-wide text-muted-foreground/70">
                  {remote ? `Another device` : `This device`}
                </div>
                <div className="rounded-lg border border-glass-stroke bg-popover p-3">
                  <Specimen doctor={fixtureCase.doctor} remote={remote} />
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  ),
}
