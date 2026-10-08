// VAPP-91 sample: a plain Vite + React app hosting an Exponential UI surface
// streamed from the local A2UI JSONL server (samples/exponential-ui/server),
// with the server's custom theme and ONE custom extension component
// (TrendLine). No Exponential account, no backend of ours: the SDK's public
// API only (`ExponentialHost` + a transport + `<HostSurface>`).
import { createRoot } from "react-dom/client"
import { useEffect, useMemo, useState } from "react"
import { ExponentialHost, JsonlStreamTransport, defineExtension } from "@exponential-at/ui"
import { HostSurface, defineReactExtension, useHostStatus } from "@exponential-at/ui-react"

const params = new URLSearchParams(location.search)
const SERVER = params.get(`server`) ?? `http://localhost:4190`
const MODE = params.get(`mode`) === `dark` ? `dark` : `light`

/** The extension's painter: an SVG polyline of the bound readings. */
function TrendLine({ props, rootProps, theme, mode }) {
  const values = Array.isArray(props.values) ? props.values.map(Number) : []
  const height = Number(props.height ?? 48)
  const color = props.color ?? theme.modes[mode].color.primary
  const min = Math.min(...values), max = Math.max(...values)
  const points = values.map((v, i) => `${(i / Math.max(values.length - 1, 1)) * 100},${max === min ? 50 : 100 - ((v - min) / (max - min)) * 100}`).join(` `)
  return (
    <div {...rootProps} style={{ height, width: `100%` }}>
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" width="100%" height={height} role="img" aria-label={`${values.length} readings`}>
        <polyline points={points} fill="none" stroke={color} strokeWidth="3" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
      </svg>
    </div>
  )
}

function Status({ host }) {
  const { status } = useHostStatus(host)
  return <p style={{ font: `12px ui-monospace, monospace`, opacity: 0.6, margin: `8px 16px` }}>transport: {status}</p>
}

function App() {
  const [assets, setAssets] = useState(null)
  useEffect(() => {
    Promise.all([fetch(`${SERVER}/extension.json`).then((r) => r.json()), fetch(`${SERVER}/theme.json`).then((r) => r.json())]).then(([extension, theme]) =>
      setAssets({ catalog: defineExtension(extension), theme })
    )
  }, [])
  const host = useMemo(
    () =>
      assets &&
      new ExponentialHost({
        transport: new JsonlStreamTransport({ url: `${SERVER}/a2ui.jsonl${params.get(`once`) ? `?once=1` : ``}`, postUrl: `${SERVER}/action` }),
        extensions: [assets.catalog],
        policy: { openUrl: (url) => window.open(url, `_blank`, `noopener`) },
      }),
    [assets]
  )
  const extensions = useMemo(() => (assets ? [defineReactExtension({ catalog: assets.catalog, components: { TrendLine } })] : []), [assets])
  useEffect(() => {
    host?.connect()
    return () => host?.close()
  }, [host])
  if (!host) return <p style={{ margin: 16 }}>Loading the theme and the extension from {SERVER}…</p>
  return (
    <main style={{ maxWidth: 480, margin: `0 auto`, padding: 16, minHeight: `100vh`, background: MODE === `dark` ? `#0b1220` : `#f8fafc` }}>
      <HostSurface host={host} surfaceId="greenhouse" theme={assets.theme} mode={MODE} extensions={extensions} fallback={<p>Waiting for the surface…</p>} />
      <Status host={host} />
    </main>
  )
}

createRoot(document.getElementById(`root`)).render(<App />)
