// FEED-128 fan-out: the Solid port of React's natives/overlays.tsx (Dialog, Drawer, Popover, Tooltip, Menu, Toast).
// Until it lands, natives/index.ts paints these with `Pending`; fill this
// record with the real painters (the keys are fixed in natives/owners.ts).

import type { Component } from "solid-js"
import type { NativeProps } from "../node-view"

export const NATIVES_OVERLAYS: Record<string, Component<NativeProps>> = {}
