// FEED-128 fan-out: the Solid port of React's natives/inputs.tsx (Input, Textarea, NumberField, Checkbox, Radio, Switch, Slider, Composer).
// Until it lands, natives/index.ts paints these with `Pending`; fill this
// record with the real painters (the keys are fixed in natives/owners.ts).

import type { Component } from "solid-js"
import type { NativeProps } from "../node-view"

export const NATIVES_INPUTS: Record<string, Component<NativeProps>> = {}
