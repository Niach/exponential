import { ExponentialSurface, createSurface } from "@exponential_app/ui-solid"
import type { A2uiMessage } from "@exponential_app/ui-solid"
import { defined } from "./defined"

// A surface written in TypeScript, painted by the Solid renderer: the
// messages defineSurface produced are the ones an agent would send.
export function Defined() {
  const surface = createSurface({ surfaceId: defined.surfaceId })
  for (const message of defined.messages) surface.apply(message as A2uiMessage)
  return <ExponentialSurface surface={surface} theme="orange" mode="dark" host={{ onAction: (event) => console.log(event) }} />
}
