// FEED-128: the native component map (React's natives/index.ts) — every
// `kind: native` of the core catalog, the SAME 48 keys. `natives.test.ts`
// gates it against the catalog. Entries painted by `Pending` are the
// fan-out's: each agent swaps its file group's entries for the real
// painters (`// FEED-128 fan-out: replaced per file`).

import type { Component } from "solid-js"
import { FormNative } from "../form"
import type { NativeProps } from "../node-view"
import { BoxNative, ListNative, UnknownNative } from "./layout"
import { Pending } from "./pending"
import { IconNative, MarkdownNative, TextNative } from "./text"

export const NATIVES: Record<string, Component<NativeProps>> = {
  Box: BoxNative,
  List: ListNative,
  Resizable: Pending, // FEED-128 fan-out: replaced per file (resizable.tsx)
  Text: TextNative,
  Markdown: MarkdownNative,
  CodeBlock: Pending, // FEED-128 fan-out: replaced per file (code.tsx)
  Image: Pending, // FEED-128 fan-out: replaced per file (media.tsx)
  Icon: IconNative,
  Video: Pending, // FEED-128 fan-out: replaced per file (media.tsx)
  AudioPlayer: Pending, // FEED-128 fan-out: replaced per file (media.tsx)
  Avatar: Pending, // FEED-128 fan-out: replaced per file (media.tsx)
  Carousel: Pending, // FEED-128 fan-out: replaced per file (media.tsx)
  Tabs: Pending, // FEED-128 fan-out: replaced per file (nav.tsx)
  Segmented: Pending, // FEED-128 fan-out: replaced per file (nav.tsx)
  Accordion: Pending, // FEED-128 fan-out: replaced per file (nav.tsx)
  Dialog: Pending, // FEED-128 fan-out: replaced per file (overlays.tsx)
  Drawer: Pending, // FEED-128 fan-out: replaced per file (overlays.tsx)
  Popover: Pending, // FEED-128 fan-out: replaced per file (overlays.tsx)
  Tooltip: Pending, // FEED-128 fan-out: replaced per file (overlays.tsx)
  Menu: Pending, // FEED-128 fan-out: replaced per file (overlays.tsx)
  Toast: Pending, // FEED-128 fan-out: replaced per file (overlays.tsx)
  Ring: Pending, // FEED-128 fan-out: replaced per file (feedback.tsx)
  Spinner: Pending, // FEED-128 fan-out: replaced per file (feedback.tsx)
  Skeleton: Pending, // FEED-128 fan-out: replaced per file (feedback.tsx)
  Table: Pending, // FEED-128 fan-out: replaced per file (table.tsx)
  Chart: Pending, // FEED-128 fan-out: replaced per file (chart.tsx)
  Button: Pending, // FEED-128 fan-out: replaced per file (actions.tsx)
  Link: Pending, // FEED-128 fan-out: replaced per file (actions.tsx)
  Toggle: Pending, // FEED-128 fan-out: replaced per file (actions.tsx)
  Form: FormNative,
  Input: Pending, // FEED-128 fan-out: replaced per file (inputs.tsx)
  Textarea: Pending, // FEED-128 fan-out: replaced per file (inputs.tsx)
  NumberField: Pending, // FEED-128 fan-out: replaced per file (inputs.tsx)
  Checkbox: Pending, // FEED-128 fan-out: replaced per file (inputs.tsx)
  Radio: Pending, // FEED-128 fan-out: replaced per file (inputs.tsx)
  ChoiceTiles: Pending, // FEED-128 fan-out: replaced per file (choice-tiles.tsx)
  Switch: Pending, // FEED-128 fan-out: replaced per file (inputs.tsx)
  Slider: Pending, // FEED-128 fan-out: replaced per file (inputs.tsx)
  Select: Pending, // FEED-128 fan-out: replaced per file (select.tsx)
  SearchList: Pending, // FEED-128 fan-out: replaced per file (search.tsx)
  ChipInput: Pending, // FEED-128 fan-out: replaced per file (chips.tsx)
  DatePicker: Pending, // FEED-128 fan-out: replaced per file (dates.tsx)
  DateRangePicker: Pending, // FEED-128 fan-out: replaced per file (dates.tsx)
  TimePicker: Pending, // FEED-128 fan-out: replaced per file (dates.tsx)
  FileUpload: Pending, // FEED-128 fan-out: replaced per file (chips.tsx)
  Composer: Pending, // FEED-128 fan-out: replaced per file (inputs.tsx)
  TreeGuides: Pending, // FEED-128 fan-out: replaced per file (feedback.tsx)
  Unknown: UnknownNative,
}

export const NATIVE_NAMES: readonly string[] = Object.keys(NATIVES)
