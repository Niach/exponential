// FEED-131: `?view=primitives&id=<specimen>&theme=&mode=&width=` — one
// primitive specimen (SPECIMENS[id], ./index.ts) on the page the web-parity
// gate compares whole-body (../../../exponential-ui/conformance/
// primitives-page.ts). The specimen function owns its state (an overlay's
// `defaultOpen`). The wrapper's style attribute is written by hand: Solid's
// compiler minifies a template's style (`padding:16px`), React writes
// `padding: 16px;`, and the DOM gate compares the attribute.
import { render } from "solid-js/web"
import { preparePrimitivesPage } from "../../../exponential-ui/conformance/primitives-page"
import { SPECIMENS } from "./index"

export async function mountPrimitives(): Promise<void> {
  const { id, container } = await preparePrimitivesPage()
  const specimen = SPECIMENS[id]
  if (!specimen) console.error(`unknown primitive specimen "${id}"`)
  render(
    () => (
      <div data-xui-specimen={id} ref={(el) => el.setAttribute(`style`, `padding: 16px;`)}>
        {specimen ? specimen() : null}
      </div>
    ),
    container
  )
}
