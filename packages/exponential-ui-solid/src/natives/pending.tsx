// FEED-128 fan-out: replaced per file. A TEMPORARY painter for every core
// native whose Solid port has not landed yet: it paints
// `<div data-xui-pending="<Component>">` (with the node's root attributes,
// so frames and ids still line up) and its children. Each fan-out agent
// replaces its entries in natives/index.ts with the real painters and
// deletes them from `PENDING_NATIVES`; when the list is empty this file goes.

import type { JSX } from "solid-js"
import type { NativeProps } from "../node-view"

export function Pending(props: NativeProps): JSX.Element {
  return (
    <div {...props.rootProps} data-xui-pending={props.node.component}>
      {props.children}
    </div>
  )
}

/** The natives still painted by `Pending` (natives.test.ts reads it). */
export const PENDING_NATIVES: readonly string[] = [
  `Resizable`,
  `CodeBlock`,
  `Image`,
  `Video`,
  `AudioPlayer`,
  `Avatar`,
  `Carousel`,
  `Tabs`,
  `Segmented`,
  `Accordion`,
  `Dialog`,
  `Drawer`,
  `Popover`,
  `Tooltip`,
  `Menu`,
  `Toast`,
  `Ring`,
  `Spinner`,
  `Skeleton`,
  `Table`,
  `Chart`,
  `Button`,
  `Link`,
  `Toggle`,
  `Input`,
  `Textarea`,
  `NumberField`,
  `Checkbox`,
  `Radio`,
  `ChoiceTiles`,
  `Switch`,
  `Slider`,
  `Select`,
  `SearchList`,
  `ChipInput`,
  `DatePicker`,
  `DateRangePicker`,
  `TimePicker`,
  `FileUpload`,
  `Composer`,
  `TreeGuides`,
]
