// The runner's whole page: its genUI surfaces and nothing else (no chrome of
// its own). `app` fills the viewport; the sheets and the toast are overlays.

import { HostSurface, useHostSurfaceIds, type HostPlugin } from "@exponential_app/eui-react"

import { runnerIcons } from "./icons"
import type { Runner } from "./runner"

const plugin: HostPlugin = { icons: runnerIcons }
const OVERLAYS = [`consent`, `permissions`, `toast`]

export function App({ runner }: { runner: Runner }) {
  const ids = useHostSurfaceIds(runner.host)
  return (
    <>
      <HostSurface host={runner.host} surfaceId="app" plugin={plugin} theme="exponential" mode="dark" style={{ height: `100%` }} />
      {OVERLAYS.filter((id) => ids.includes(id)).map((id) => (
        <HostSurface key={id} host={runner.host} surfaceId={id} plugin={plugin} theme="exponential" mode="dark" />
      ))}
    </>
  )
}
