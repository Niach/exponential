import { useEffect, useMemo } from "react"
import { A2UI_VERSION, CORE_CATALOG_ID, ExponentialHost, SseTransport } from "@exponential_app/eui"
import { HostSurface } from "@exponential_app/eui-react"

// guides/agent: `GET /express` replays a streamed model answer in Express.
const AGENT = `http://localhost:4300`

/** The prose is yours: the SDK hands it to `onText` and draws none of it.
 *  Here it goes into two surfaces the app owns (a Markdown each): the run
 *  before the block above the agent's surface, the run after it below. */
function proseSurfaces(host: ExponentialHost) {
  for (const surfaceId of [`intro`, `outro`])
    host.receive({
      version: A2UI_VERSION,
      createSurface: { surfaceId, catalogId: CORE_CATALOG_ID, components: [{ id: `root`, component: `Markdown`, text: { "@path": `/text` } }], dataModel: { text: `` } },
    })
  let target = `intro`
  let run = ``
  return (piece: string) => {
    // A "\n\n" piece = a block passed: the next run starts.
    if (piece === `\n\n`) return void ((target = `outro`), (run = ``))
    run += piece
    host.receive({ version: A2UI_VERSION, updateDataModel: { surfaceId: target, path: `/text`, value: run } })
  }
}

export function ExpressStream() {
  const host = useMemo(() => {
    let onText = (_: string) => {}
    const host = new ExponentialHost({
      // `format: "express"`: the agent writes Express (prose around
      // `<a2ui>` blocks); the compiler loads before the first byte.
      transport: new SseTransport({ url: `${AGENT}/express${location.search}`, postUrl: `${AGENT}/action`, format: `express` }),
      onText: (piece) => onText(piece),
    })
    onText = proseSurfaces(host)
    return host
  }, [])
  useEffect(() => {
    host.connect()
    return () => host.close()
  }, [host])

  return (
    <div style={{ display: `flex`, flexDirection: `column`, gap: 16 }}>
      <HostSurface host={host} surfaceId="intro" theme="exponential" mode="dark" />
      <HostSurface host={host} surfaceId="main" theme="exponential" mode="dark" />
      <HostSurface host={host} surfaceId="outro" theme="exponential" mode="dark" />
    </div>
  )
}
