// FEED-128 fan-out: replaced per file. A TEMPORARY painter for every core
// native whose Solid port has not landed yet: it paints
// `<div data-xui-pending="<Component>">` (with the node's root attributes,
// so frames and ids still line up) and its children. Each fan-out lane fills
// its own natives file's record (owners.ts); `PENDING_NATIVES` (index.ts)
// is what is left; when it is empty this file goes.

import type { JSX } from "solid-js"
import type { NativeProps } from "../node-view"

export function Pending(props: NativeProps): JSX.Element {
  return (
    <div {...props.rootProps} data-xui-pending={props.node.component}>
      {props.children}
    </div>
  )
}
