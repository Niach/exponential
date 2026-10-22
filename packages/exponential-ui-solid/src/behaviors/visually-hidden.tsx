// FEED-131: Radix `VisuallyHidden`.

import type { JSX } from "solid-js"
import { styleWriter } from "./style-writer"

/** Radix `VisuallyHidden` with `role="tooltip"` (its inline style, in its order). */
export function VisuallyHidden(props: { id: string; children: JSX.Element }): JSX.Element {
  const el = (
    <span id={props.id} role="tooltip">
      {props.children}
    </span>
  ) as HTMLSpanElement
  styleWriter(el)({ position: `absolute`, border: `0`, width: `1px`, height: `1px`, padding: `0`, margin: `-1px`, overflow: `hidden`, clip: `rect(0, 0, 0, 0)`, "white-space": `nowrap`, "word-wrap": `normal` })
  return el
}
