import { act, fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { TooltipProvider } from "@exp/ui"
import { MyMachines } from "@/components/my-machines"
import type { SteerDevice } from "@/lib/steer-devices"

// #EXP-2126 (sidebar-menus.json `phone`): a device card's long-press /
// right-click menu on the phones' Devices list.

const rename = vi.hoisted(() => vi.fn(async () => ({})))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: { devices: { rename: { mutate: rename } } } }))
vi.mock(`@/components/device-settings-dialog`, () => ({
  DeviceSettingsDialog: ({ open }: { open: boolean }) => (open ? <div data-testid="device-settings" /> : null),
}))
vi.mock(`@/components/add-device-dialog`, () => ({ AddDeviceDialog: () => null }))
vi.mock(`@/components/device-logins`, () => ({
  DeviceAccountIcons: () => null,
  loginLimitHit: () => false,
  useDeviceLogins: () => [],
}))
vi.mock(`@/hooks/use-agent-usage-refresh`, () => ({ useAgentUsageRefresh: () => {} }))

const device = (patch: Partial<SteerDevice>) =>
  ({ deviceId: `d1`, deviceLabel: `Studio`, kind: `desktop`, registered: true, online: true, agents: [], caps: [], ...patch }) as unknown as SteerDevice

const open = async (id: string) => {
  await act(async () => {
    fireEvent.contextMenu(screen.getByTestId(`device-row-${id}`))
  })
  return screen.getAllByRole(`menuitem`).map((item) => item.textContent)
}

describe(`MyMachines menus`, () => {
  beforeEach(() => rename.mockClear())
  const mount = () =>
    render(
      <TooltipProvider>
        <MyMachines
          devices={[device({}), device({ deviceId: `d2`, deviceLabel: `Server`, kind: `server`, owner: { id: `u2`, name: `Jo` } as never })]}
          onChanged={() => {}}
          latestVersions={null}
        />
      </TooltipProvider>
    )

  it(`mine: Rename in place, Copy link, Settings`, async () => {
    mount()
    expect(await open(`d1`)).toEqual([`Rename`, `Copy link`, `Settings`])
    await act(async () => {
      fireEvent.click(screen.getByTestId(`sidebar-menu-rename`))
      await new Promise((resolve) => setTimeout(resolve, 20))
    })
    const field = screen.getByTestId(`device-name`)
    fireEvent.change(field, { target: { value: `Desk` } })
    fireEvent.keyDown(field, { key: `Enter` })
    expect(rename).toHaveBeenCalledWith({ deviceId: `d1`, label: `Desk` }, expect.anything())
  })

  it(`a teammate's: Copy link alone`, async () => {
    mount()
    expect(await open(`d2`)).toEqual([`Copy link`])
  })
})
