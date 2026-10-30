// FEED-128: the native component map (React's natives/index.ts): every
// `kind: native` of the core catalog, the SAME 51 keys in React's order
// (`owners.ts` NATIVE_ORDER). Each natives file exports its own record;
// a native no file paints yet is painted by `Pending` (natives.test.ts
// gates the map against the catalog and the owners table).

import type { Component } from "solid-js"
import { FormNative } from "../form"
import type { NativeProps } from "../node-view"
import { BoxNative, ListNative, UnknownNative } from "./layout"
import { NATIVE_ORDER } from "./owners"
import { Pending } from "./pending"
import { IconNative, MarkdownNative, TextNative } from "./text"
import { NATIVES_ACTIONS } from "./actions"
import { NATIVES_CHART } from "./chart"
import { NATIVES_CHIPS } from "./chips"
import { NATIVES_CHOICE_TILES } from "./choice-tiles"
import { NATIVES_CODE } from "./code"
import { NATIVES_DATES } from "./dates"
import { NATIVES_FEEDBACK } from "./feedback"
import { NATIVES_INPUTS } from "./inputs"
import { NATIVES_MEDIA } from "./media"
import { NATIVES_NAV } from "./nav"
import { NATIVES_OVERLAYS } from "./overlays"
import { NATIVES_QR } from "./qr"
import { NATIVES_RESIZABLE } from "./resizable"
import { NATIVES_SEARCH } from "./search"
import { NATIVES_SELECT } from "./select"
import { NATIVES_TABLE } from "./table"

const PROVIDED: Record<string, Component<NativeProps>> = {
  Box: BoxNative,
  List: ListNative,
  Unknown: UnknownNative,
  Text: TextNative,
  Markdown: MarkdownNative,
  Icon: IconNative,
  Form: FormNative,
  ...NATIVES_ACTIONS,
  ...NATIVES_CHART,
  ...NATIVES_CHIPS,
  ...NATIVES_CHOICE_TILES,
  ...NATIVES_CODE,
  ...NATIVES_DATES,
  ...NATIVES_FEEDBACK,
  ...NATIVES_INPUTS,
  ...NATIVES_MEDIA,
  ...NATIVES_NAV,
  ...NATIVES_OVERLAYS,
  ...NATIVES_QR,
  ...NATIVES_RESIZABLE,
  ...NATIVES_SEARCH,
  ...NATIVES_SELECT,
  ...NATIVES_TABLE,
}

export const NATIVES: Record<string, Component<NativeProps>> = Object.fromEntries(NATIVE_ORDER.map((name) => [name, PROVIDED[name] ?? Pending]))

export const NATIVE_NAMES: readonly string[] = Object.keys(NATIVES)

/** The natives still painted by `Pending` (empty once the fan-out landed). */
export const PENDING_NATIVES: readonly string[] = NATIVE_ORDER.filter((name) => !PROVIDED[name])
