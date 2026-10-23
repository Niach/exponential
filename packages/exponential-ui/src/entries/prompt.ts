// VAPP-106: `@exponential_app/ui/prompt`, the model-facing prompt as its own
// entry. The root entry ships the catalog WITHOUT its descriptions (a browser
// that renders never needs them); this entry attaches them
// (`describeCatalog`) before the prompt reads them. The prompt itself is
// src/prompt.ts, which this file leaves alone.

import { describeCatalog } from "../catalog.described"
import { CHARS_PER_TOKEN, COMMON_PROPS, DEFAULT_SECTIONS, PROMPT_RULES, PROMPT_SECTIONS, catalogIndex as bareCatalogIndex, catalogPrompt as bareCatalogPrompt, componentPrompt as bareComponentPrompt, estimateTokens } from "../prompt"
import type { PromptOptions, PromptSection } from "../prompt"

export { CHARS_PER_TOKEN, COMMON_PROPS, DEFAULT_SECTIONS, PROMPT_RULES, PROMPT_SECTIONS, describeCatalog, estimateTokens }
export type { PromptOptions, PromptSection }

/** The catalog prompt (src/prompt.ts `catalogPrompt`) over the described catalog. */
export function catalogPrompt(options: PromptOptions = {}): string {
  describeCatalog()
  return bareCatalogPrompt(options)
}

/** One component's prompt section (VAPP-108, the on-demand per-component prompt) over the described catalog. */
export function componentPrompt(name: string, options: Parameters<typeof bareComponentPrompt>[1] = {}): string {
  describeCatalog()
  return bareComponentPrompt(name, options)
}

/** The one-line-per-component index (VAPP-108) over the described catalog. */
export function catalogIndex(options: Parameters<typeof bareCatalogIndex>[0] = {}): string[] {
  describeCatalog()
  return bareCatalogIndex(options)
}
