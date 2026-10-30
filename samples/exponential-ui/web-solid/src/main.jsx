// FEED-128 sample: the web sample (../web) on SolidJS: a plain Vite + Solid
// app hosting an Exponential UI surface streamed from the local A2UI JSONL
// server (samples/exponential-ui/server), with the server's custom theme and
// ONE custom extension component (TrendLine). No Exponential account, no
// backend of ours: the SDK's public API only (`ExponentialHost` + a
// transport + `<HostSurface>`).
import { render } from "solid-js/web"
import { Show, createMemo, createResource, onCleanup } from "solid-js"
import { ExponentialHost, JsonlStreamTransport, defineExtension } from "@exponential_app/eui"
import { HostSurface, createHostStatus, defineSolidExtension } from "@exponential_app/eui-solid"

const params = new URLSearchParams(location.search)
const SERVER = params.get(`server`) ?? `http://localhost:4190`
const MODE = params.get(`mode`) === `dark` ? `dark` : `light`

/** The extension's painter: an SVG polyline of the bound readings. Solid
 *  props are live getters: read them in functions, never destructure. */
function TrendLine(props) {
  const values = () => (Array.isArray(props.props.values) ? props.props.values.map(Number) : [])
  const height = () => Number(props.props.height ?? 48)
  const color = () => props.props.color ?? props.theme.modes[props.mode].color.primary
  const points = () => {
    const v = values()
    const min = Math.min(...v), max = Math.max(...v)
    return v.map((n, i) => `${(i / Math.max(v.length - 1, 1)) * 100},${max === min ? 50 : 100 - ((n - min) / (max - min)) * 100}`).join(` `)
  }
  return (
    <div {...props.rootProps} style={{ height: `${height()}px`, width: `100%` }}>
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" width="100%" height={height()} role="img" aria-label={`${values().length} readings`}>
        <polyline points={points()} fill="none" stroke={color()} stroke-width="3" vector-effect="non-scaling-stroke" stroke-linejoin="round" />
      </svg>
    </div>
  )
}

function Status(props) {
  const status = createHostStatus(props.host)
  return <p style={{ font: `12px ui-monospace, monospace`, opacity: 0.6, margin: `8px 16px` }}>transport: {status.status}</p>
}

function Sample(props) {
  const host = new ExponentialHost({
    transport: new JsonlStreamTransport({ url: `${SERVER}/a2ui.jsonl${params.get(`once`) ? `?once=1` : ``}`, postUrl: `${SERVER}/action` }),
    extensions: [props.assets.catalog],
    policy: { openUrl: (url) => window.open(url, `_blank`, `noopener`) },
  })
  const extensions = [defineSolidExtension({ catalog: props.assets.catalog, components: { TrendLine } })]
  host.connect()
  onCleanup(() => host.close())
  return (
    <main style={{ "max-width": `480px`, margin: `0 auto`, padding: `16px`, "min-height": `100vh`, background: MODE === `dark` ? `#0b1220` : `#f8fafc` }}>
      <HostSurface host={host} surfaceId="greenhouse" theme={props.assets.theme} mode={MODE} extensions={extensions} fallback={<p>Waiting for the surface…</p>} />
      <Status host={host} />
    </main>
  )
}

function App() {
  const [assets] = createResource(async () => {
    const [extension, theme] = await Promise.all([fetch(`${SERVER}/extension.json`).then((r) => r.json()), fetch(`${SERVER}/theme.json`).then((r) => r.json())])
    return { catalog: defineExtension(extension), theme }
  })
  const ready = createMemo(() => assets())
  return (
    <Show when={ready()} keyed fallback={<p style={{ margin: `16px` }}>Loading the theme and the extension from {SERVER}…</p>}>
      {(a) => <Sample assets={a} />}
    </Show>
  )
}

render(() => <App />, document.getElementById(`root`))
