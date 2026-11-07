import { useEffect, useMemo, useState } from "react"
import { ExponentialHost, JsonlStreamTransport } from "@exponential_app/eui"
import { HostSurface } from "@exponential_app/eui-react"
import { Button } from "@exponential_app/eui-react/primitives"
import "@exponential_app/eui-react/styles.css"

export function App() {
  // One host per connection: A2UI messages in over a transport, actions back
  // out. A surface id lives once per host, so a reconnect that replays the
  // agent's createSurface needs a new host.
  const [connection, setConnection] = useState(0)
  const host = useMemo(
    () => new ExponentialHost({ transport: new JsonlStreamTransport({ url: `/a2ui.jsonl`, postUrl: `/action` }) }),
    [connection]
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
      <Button variant="outline" onClick={() => setConnection((n) => n + 1)}>
        Reconnect
      </Button>
    </>
  )
}
