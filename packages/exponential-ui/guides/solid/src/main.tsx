import { Show, createSignal } from "solid-js"
import { render } from "solid-js/web"
import { App } from "./App"
import { ExpressStream } from "./ExpressStream"

// `#express`: the Express stream (a model's prose + the UI it writes).
const [hash, setHash] = createSignal(location.hash)
addEventListener(`hashchange`, () => setHash(location.hash))

render(
  () => (
    <Show when={hash() === `#express`} fallback={<App />}>
      <ExpressStream />
    </Show>
  ),
  document.getElementById(`root`)!
)
