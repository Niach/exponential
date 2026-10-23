// FEED-131: `?view=primitives&id=<specimen>&theme=&mode=&width=` — one
// primitive specimen (SPECIMENS[id], ./index.ts) on the page the web-parity
// gate compares whole-body (../../../exponential-ui/conformance/
// primitives-page.ts). The specimen is a COMPONENT: it may hold state
// (an overlay's `defaultOpen`).
import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { preparePrimitivesPage } from "../../../exponential-ui/conformance/primitives-page"
import { SPECIMENS } from "./index"

export async function mountPrimitives(): Promise<void> {
  const { id, container } = await preparePrimitivesPage()
  const Specimen = SPECIMENS[id]
  if (!Specimen) console.error(`unknown primitive specimen "${id}"`)
  createRoot(container).render(
    <StrictMode>
      <div data-xui-specimen={id} style={{ padding: 16 }}>
        {Specimen ? <Specimen /> : null}
      </div>
    </StrictMode>
  )
}
