// Registers <exponential-surface>: no framework, the element paints itself.
import "@exponential_app/eui-element/define"
import { A2UI_VERSION, CORE_CATALOG_ID } from "@exponential_app/eui"
import type { ExponentialSurfaceElement } from "@exponential_app/eui-element"
import { metrics } from "./TrendLine"

const el = document.querySelector<ExponentialSurfaceElement>(`exponential-surface`)!
el.extensions = [metrics]

// A2UI 1.0 server messages in; the element keeps the tree and the data.
el.apply({
  version: A2UI_VERSION,
  createSurface: {
    surfaceId: `main`,
    catalogId: CORE_CATALOG_ID,
    components: [
      { id: `root`, component: `Column`, children: [`title`, `body`] },
      { id: `title`, component: `Text`, text: `Hello from the agent`, variant: `title` },
      { id: `body`, component: `Text`, text: { "@path": `/status` } },
    ],
    dataModel: { status: `Waiting for the agent…` },
  },
})

// Messages from the agent, here one JSON message per line over SSE.
const events = new EventSource(`/a2ui`)
events.onmessage = (e) => el.apply(JSON.parse(e.data))

// Actions go to your agent; waitUntil keeps the control pending until it answers.
el.addEventListener(`xui-action`, (e) => {
  const { detail } = e as CustomEvent
  detail.waitUntil(fetch(`/action`, { method: `POST`, body: JSON.stringify(detail) }))
})
