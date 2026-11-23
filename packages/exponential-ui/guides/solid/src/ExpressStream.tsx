import { onCleanup, onMount } from "solid-js"
import { A2UI_VERSION, CORE_CATALOG_ID, ExponentialHost, SseTransport } from "@exponential_app/eui"
import { HostSurface } from "@exponential_app/eui-solid"

// guides/agent: `GET /express` replays a streamed model answer in Express.
const AGENT = `http://localhost:4300`

/** The prose is yours: the SDK hands it to `onText` and draws none of it.
 *  Here it goes into two surfaces the app owns (a Markdown each): the run
 *  before the block above the agent's surface, the run after it below. */
function proseSurfaces(host: ExponentialHost): (piece: string) => void {
  for (const surfaceId of [`intro`, `outro`])
    host.receive({
      version: A2UI_VERSION,
      createSurface: { surfaceId, catalogId: CORE_CATALOG_ID, components: [{ id: `root`, component: `Markdown`, text: { "@path": `/text` } }], dataModel: { text: `` } },
    })
  let target = `intro`
  let run = ``
  return (piece) => {
    // A "\n\n" piece = a block passed: the next run starts.
    if (piece === `\n\n`) return void ((target = `outro`), (run = ``))
    run += piece
    host.receive({ version: A2UI_VERSION, updateDataModel: { surfaceId: target, path: `/text`, value: run } })
  }
}

export function ExpressStream() {
  // One host per stream; `onText` gets the prose a piece at a time.
  let prose: ((piece: string) => void) | undefined // the prose surfaces, from the first piece on
  const host: ExponentialHost = new ExponentialHost({
    // `format: "express"`: the agent writes Express (prose around
    // `<a2ui>` blocks); the compiler loads before the first byte.
    transport: new SseTransport({ url: `${AGENT}/express${location.search}`, postUrl: `${AGENT}/action`, format: `express` }),
    onText: (piece) => (prose ??= proseSurfaces(host))(piece),
  })
  onMount(() => host.connect())
  onCleanup(() => host.close())

  return (
    <div style={{ display: `flex`, "flex-direction": `column`, gap: `16px`, padding: `16px` }}>
      <HostSurface host={host} surfaceId="intro" theme="exponential" mode="dark" />
      <HostSurface host={host} surfaceId="main" theme="exponential" mode="dark" />
      <HostSurface host={host} surfaceId="outro" theme="exponential" mode="dark" />
    </div>
  )
}
