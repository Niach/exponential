import { useEffect, useMemo } from "react"
import { ExponentialHost, JsonlStreamTransport } from "@exponential-at/ui"
import { HostSurface } from "@exponential-at/ui-react"

export function App() {
  // One host per app: A2UI messages in over a transport, actions back out.
  const host = useMemo(
    () => new ExponentialHost({ transport: new JsonlStreamTransport({ url: `/a2ui.jsonl`, postUrl: `/action` }) }),
    []
  )
  useEffect(() => {
    host.connect()
    return () => host.close()
  }, [host])

  // Paints the surface the agent creates; the theme is data, the mode a prop.
  return <HostSurface host={host} surfaceId="main" theme="exponential" mode="dark" fallback={<p>Waiting for the agent…</p>} />
}
