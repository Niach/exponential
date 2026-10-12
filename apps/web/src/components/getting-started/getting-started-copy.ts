import spec from "@exp/domain-contract/fixtures/getting-started.json"
import type { GettingStartedStepKey } from "@/components/getting-started/getting-started-model"

// EXP-1270 (EXP-1259 D6): Getting started's words are the fixture's
// (`packages/domain-contract/fixtures/getting-started.json`), the ONE source
// all four clients port: the block's title, its `{done} of {total}`, the Docs
// button, and ONE title per card. Nothing else: no descriptions, no
// per-card buttons, no locked hints. `getting-started-copy.test.ts` pins this
// table to the fixture.

export const GETTING_STARTED_TITLE = spec.title
export const GETTING_STARTED_DOCS = spec.docs

/** The card titles, by step. */
export const GETTING_STARTED_STEP_TITLES = Object.fromEntries(
  spec.steps.map((step) => [step.key, step.title])
) as Record<GettingStartedStepKey, string>

/** The device-setup block's two cards (`device-setup.tsx`, EXP-1169: the
 *  wizard's devices step, Add device, the readiness fix). They read their
 *  words from here since the old checklist owned them; they are the device
 *  block's copy now, not Getting started's. */
export const DEVICE_CARD_COPY = {
  desktop: {
    title: `Get the desktop app`,
    description: `Runs agents on your machine and registers it as one of your devices.`,
    action: `Download the desktop app`,
  },
  server: {
    title: `Set up a server`,
    description: `Run the headless daemon on an always-on machine to take remote Start coding requests.`,
    action: `Copy install command`,
  },
} as const

/** @deprecated The checklist's per-entry table is gone (EXP-1270); only the
 *  device-setup block still reads it by this name. Use `DEVICE_CARD_COPY`. */
export const GETTING_STARTED_COPY = DEVICE_CARD_COPY
