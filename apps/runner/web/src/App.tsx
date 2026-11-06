// The runner's whole page: its genUI surfaces and nothing else (no chrome of
// its own). `app` fills the viewport; the sheets and the toast are overlays.

import { useMemo } from "react"
import { HostSurface, useHostSurfaceIds, type HostPlugin, type IconMap } from "@exponential_app/eui-react"

import type { Runner } from "./runner"

const OVERLAYS = [`consent`, `permissions`, `toast`]

/** `icons`: the export's glyphs (src/glyphs.ts). */
export function App({ runner, icons }: { runner: Runner; icons: IconMap }) {
  const plugin: HostPlugin = useMemo(() => ({ icons }), [icons])
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
