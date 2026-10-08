// VAPP-92: the standalone theme builder page (`dev:builder` / `build:builder`).
// The builder itself is mount.ts; ui.exponential.at mounts the same module
// into its /themes/builder/ page (VAPP-93).

import { mountBuilder } from "./mount"

mountBuilder(document, document.body)
