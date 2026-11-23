// The Express stream: a model's answer written in Express (prose around an
// `<a2ui>` block), read with `format: "express"`. guides/agent `GET /express`
// replays one.
import "@exponential_app/eui-element/define"
import { A2UI_VERSION, CORE_CATALOG_ID, ExponentialHost, SseTransport } from "@exponential_app/eui"
import type { ExponentialSurfaceElement } from "@exponential_app/eui-element"

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

export function mountExpressStream(parent: HTMLElement): ExponentialHost {
  let prose: ((piece: string) => void) | undefined // the prose surfaces, from the first piece on
  const host: ExponentialHost = new ExponentialHost({
    // `format: "express"`: the compiler loads before the first byte.
    transport: new SseTransport({ url: `${AGENT}/express${location.search}`, postUrl: `${AGENT}/action`, format: `express` }),
    onText: (piece) => (prose ??= proseSurfaces(host))(piece),
  })
  const column = document.createElement(`div`)
  column.style.cssText = `display: flex; flex-direction: column; gap: 16px; padding: 16px`
  for (const surfaceId of [`intro`, `main`, `outro`]) {
    // One element per surface, all painting the same host.
    const el = document.createElement(`exponential-surface`) as ExponentialSurfaceElement
    el.setAttribute(`surface-id`, surfaceId)
    el.setAttribute(`theme`, `exponential`)
    el.setAttribute(`mode`, `dark`)
    el.host = host
    column.append(el)
  }
  parent.append(column)
  host.connect()
  return host
}
