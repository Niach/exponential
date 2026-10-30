// FEED-130: what `<exponential-surface>` tells the page. Every renderer
// callback of the host plugin is ALSO a DOM event on the element (a
// `CustomEvent`, bubbling and composed), so a framework-less page listens
// instead of building a plugin. The ones the painter waits on (`xui-action`,
// `xui-input`, `xui-upload`) carry `waitUntil(promise)`: the plugin's promise
// and every listener's are JOINED, so the control stays pending until all
// settle. The ones that need a return value (option sources, media,
// resolveUrl, Markdown…) stay plugin-only.

import type { FunctionCallInfo, ThemeIssue, TransportStatus, UiNode } from "@exponential_app/eui"
import type { PaintError, SurfaceActionEvent, SurfaceHandle, SurfaceInputEvent } from "@exponential_app/eui-solid"
import type { ExponentialSurfaceElement } from "./element"

/** Keeps the source control pending until `promise` settles. */
export interface WaitUntil {
  waitUntil: (promise: Promise<unknown>) => void
}

export type SurfaceActionDetail = SurfaceActionEvent & WaitUntil
/** `waitUntil` acknowledges the input's revision when it settles. */
export type SurfaceInputDetail = SurfaceInputEvent & WaitUntil
export interface FunctionCallDetail {
  call: FunctionCallInfo
  /** The call's result (the first `respond` wins; none = the plugin's
   *  `onFunctionCall` result, else undefined). A promise keeps the source
   *  control pending. */
  respond: (value: unknown) => void
}
/** Cancelable: `preventDefault()` = the page opens it (or not) itself. */
export interface OpenUrlDetail {
  url: string
}
export interface UploadDetail extends WaitUntil {
  files: File[]
  target: { nodeId: string; name: string }
}
export interface StatusDetail {
  status: TransportStatus
  detail?: string
}

export interface ExponentialSurfaceEventMap {
  "xui-action": CustomEvent<SurfaceActionDetail>
  "xui-input": CustomEvent<SurfaceInputDetail>
  "xui-function-call": CustomEvent<FunctionCallDetail>
  "xui-open-url": CustomEvent<OpenUrlDetail>
  "xui-upload": CustomEvent<UploadDetail>
  "xui-paint-error": CustomEvent<{ error: PaintError }>
  "xui-theme-issues": CustomEvent<{ issues: ThemeIssue[] }>
  "xui-unknown": CustomEvent<{ node: UiNode }>
  "xui-status": CustomEvent<StatusDetail>
  "xui-ready": CustomEvent<{ handle: SurfaceHandle }>
}

export type ExponentialSurfaceEventName = keyof ExponentialSurfaceEventMap

declare global {
  interface HTMLElementTagNameMap {
    "exponential-surface": ExponentialSurfaceElement
  }
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  interface HTMLElementEventMap extends ExponentialSurfaceEventMap {}
}

type DetailOf<K extends ExponentialSurfaceEventName> = ExponentialSurfaceEventMap[K] extends CustomEvent<infer D> ? D : never

/** Dispatch one event; returns false when a listener cancelled it. */
export function emit<K extends ExponentialSurfaceEventName>(target: EventTarget, name: K, detail: DetailOf<K>, cancelable = false): boolean {
  return target.dispatchEvent(new CustomEvent(name, { detail, bubbles: true, composed: true, cancelable }))
}

const isThenable = (v: unknown): v is PromiseLike<unknown> => typeof v === `object` && v !== null && typeof (v as { then?: unknown }).then === `function`

/** The plugin's result and every `waitUntil` promise, joined: undefined when
 *  none is a promise (the painter acknowledges at once), else one promise
 *  that settles with all of them (and rejects with the first rejection). */
export function joinPending(results: readonly unknown[]): Promise<void> | undefined {
  const pending = results.filter(isThenable)
  if (pending.length === 0) return undefined
  return Promise.all(pending).then(() => undefined)
}

/** Dispatch a waiting event: `base` + `waitUntil`; returns the promises the
 *  listeners handed over. */
export function emitWaiting<K extends `xui-action` | `xui-input` | `xui-upload`>(target: EventTarget, name: K, base: Omit<DetailOf<K>, `waitUntil`>): Promise<unknown>[] {
  const waits: Promise<unknown>[] = []
  const detail = { ...base, waitUntil: (p: Promise<unknown>) => void waits.push(Promise.resolve(p)) } as DetailOf<K>
  emit(target, name, detail)
  return waits
}
