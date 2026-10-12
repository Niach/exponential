import spec from "@exp/domain-contract/fixtures/getting-started.json"
import type { GettingStartedStepKey } from "@/components/getting-started/getting-started-model"

// EXP-1270 (EXP-1259 D6): Getting started's words are the fixture's
// (`packages/domain-contract/fixtures/getting-started.json`), the ONE source
// all four clients port: the block's title, its `{done} of {total}`, the Docs
// button, the expanded row's `Next` label, and per step ONE title, ONE `why`
// sentence and ONE button label (`cta`). Nothing else: no numbers, no locked
// hints. `getting-started-copy.test.ts` pins this table to the fixture.

export const GETTING_STARTED_TITLE = spec.title
export const GETTING_STARTED_DOCS = spec.docs
export const GETTING_STARTED_NEXT_LABEL = spec.nextLabel
/** The block's widest (the sidebar dialog is exactly this wide). */
export const GETTING_STARTED_MAX_WIDTH = spec.maxWidth

/** The row titles, by step. */
export const GETTING_STARTED_STEP_TITLES = Object.fromEntries(
  spec.steps.map((step) => [step.key, step.title])
) as Record<GettingStartedStepKey, string>

/** The expanded row's sentence, by step. */
export const GETTING_STARTED_STEP_WHY = Object.fromEntries(
  spec.steps.map((step) => [step.key, step.why])
) as Record<GettingStartedStepKey, string>

/** The expanded row's primary button label, by step. */
export const GETTING_STARTED_STEP_CTA = Object.fromEntries(
  spec.steps.map((step) => [step.key, step.cta])
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
