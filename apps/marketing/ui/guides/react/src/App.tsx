import { useEffect, useMemo } from "react"
import { ExponentialHost, JsonlStreamTransport } from "@exponential_app/eui"
import { HostSurface } from "@exponential_app/eui-react"
import { Button } from "@exponential_app/eui-react/primitives"
import "@exponential_app/eui-react/styles.css"

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
  // Below it, a primitive from the same set (the stylesheet above styles it).
  return (
    <>
      <HostSurface host={host} surfaceId="main" theme="exponential" mode="dark" fallback={<p>Waiting for the agent…</p>} />
      <Button variant="outline" onClick={() => { host.close(); host.connect() }}>
        Reconnect
      </Button>
    </>
  )
}
