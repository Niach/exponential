import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1629 — THE shortcut field: Exponential UI's `KeyRecorder` macro
// (fixtures/interactions-round1.json `keyRecorder`), painted by the real
// React renderer from fixtures/specimens.json `exponential-ui-key-recorder`:
// a binding, none, recording, a held modifier, an inline error.

export const entry: StyleguideEntry = {
  id: `key-recorder`,
  section: `general`,
  owner: `EXP-1629`,
  title: `Key recorder`,
  blurb: `A shortcut field: Exponential UI's KeyRecorder. Idle it shows the binding as key caps (None while empty); a press starts recording: the ring border, the placeholder, the held modifiers as caps and an ellipsis. While it records every key down and up, modifier change and mouse button 3 to 5 goes to the host as key, which owns the grammar and writes the binding; start and stop let it pause its own shortcut grab, and losing the focus stops. An error shows under the field.`,
  status: {
    web: { state: `ok`, symbol: `KeyRecorder (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/key_recorder.rs` },
    ios: { state: `ok`, symbol: `KeyRecorder (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `KeyRecorder (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-key-recorder" />,
}
