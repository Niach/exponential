// A surface written in TypeScript, painted by <exponential-surface>: the
// messages defineSurface produced are the ones an agent would send.
import "@exponential_app/ui-element/define"
import type { ExponentialSurfaceElement } from "@exponential_app/ui-element"
import { defined } from "./defined"

export function mountDefined(parent: HTMLElement): ExponentialSurfaceElement {
  const el = document.createElement(`exponential-surface`) as ExponentialSurfaceElement
  el.setAttribute(`surface-id`, defined.surfaceId)
  el.setAttribute(`theme`, `orange`)
  el.setAttribute(`mode`, `dark`)
  parent.append(el)
  for (const message of defined.messages) el.apply(message as Parameters<ExponentialSurfaceElement[`apply`]>[0])
  el.addEventListener(`xui-action`, (e) => console.log((e as CustomEvent).detail))
  return el
}
