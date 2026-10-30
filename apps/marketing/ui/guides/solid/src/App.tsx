import { onCleanup, onMount } from "solid-js"
import { CORE_CATALOG_ID } from "@exponential_app/eui"
import { ExponentialSurface, createSurface } from "@exponential_app/eui-solid"
import type { A2uiMessage } from "@exponential_app/eui-solid"
import { Button } from "@exponential_app/eui-solid/primitives"
import "@exponential_app/eui-solid/styles.css"

export function App() {
  // The surface state: A2UI v0.9 server messages in, the tree and data out.
  const surface = createSurface({ surfaceId: `main` })

  // Messages from the agent, here one JSON message per line over SSE.
  onMount(() => {
    surface.apply({ createSurface: { surfaceId: `main`, catalogId: CORE_CATALOG_ID } })
    surface.apply({
      updateComponents: {
        surfaceId: `main`,
        components: [
          { id: `root`, component: `Column`, children: [`title`, `body`] },
          { id: `title`, component: `Text`, text: `Hello from the agent`, variant: `title` },
          { id: `body`, component: `Text`, text: { path: `/status` } },
        ],
      },
    })
    surface.apply({ updateDataModel: { surfaceId: `main`, path: `/status`, value: `Waiting for the agent…` } })
    const events = new EventSource(`/a2ui`)
    events.onmessage = (e) => surface.apply(JSON.parse(e.data) as A2uiMessage)
    onCleanup(() => events.close())
  })

  // Paints it; the theme is data, the mode a prop. Actions go to your agent.
  // Below it, a primitive from the same set (the stylesheet above styles it).
  return (
    <>
      <ExponentialSurface
        surface={surface}
        theme="orange"
        mode="dark"
        host={{ onAction: (event) => void fetch(`/action`, { method: `POST`, body: JSON.stringify(event) }) }}
      />
      <Button variant="outline" onClick={() => surface.apply({ updateDataModel: { surfaceId: `main`, path: `/status`, value: `Asked again` } })}>
        Ask again
      </Button>
    </>
  )
}
