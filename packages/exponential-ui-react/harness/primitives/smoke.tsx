// FEED-131: the gate's own SMOKE specimen — plain elements wearing classes
// of the compiled stylesheet: proves the sheet and the shadcn variables
// (globalsCss) are live and paint identically on both harnesses. Twin:
// packages/exponential-ui-solid/harness/primitives/smoke.tsx.
import type { ReactElement } from "react"

export const SPECIMENS: Record<string, () => ReactElement> = {
  "smoke/button": () => (
    <>
      <button type="button" data-xui-target="" className="inline-flex h-9 items-center justify-center gap-2 rounded-md border border-input px-4 py-2 text-sm font-medium shadow-xs hover:bg-accent">
        Smoke button
      </button>
      <div className="bg-primary text-primary-foreground rounded-md p-4">Primary block</div>
    </>
  ),
}
