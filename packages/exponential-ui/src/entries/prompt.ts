// VAPP-106: `@exponential-at/ui/prompt`, the model-facing prompt as its own
// entry. The root entry ships the catalog WITHOUT its descriptions (a browser
// that renders never needs them); this entry attaches them
// (`describeCatalog`) before the prompt reads them. The prompt itself is
// src/prompt.ts, which this file leaves alone.

import { describeCatalog } from "../catalog.described"
import { CHARS_PER_TOKEN, COMMON_PROPS, PROMPT_RULES, catalogPrompt as bareCatalogPrompt, estimateTokens } from "../prompt"
import type { PromptOptions } from "../prompt"

export { CHARS_PER_TOKEN, COMMON_PROPS, PROMPT_RULES, describeCatalog, estimateTokens }
export type { PromptOptions }

/** The catalog prompt (src/prompt.ts `catalogPrompt`) over the described catalog. */
export function catalogPrompt(options: PromptOptions = {}): string {
  describeCatalog()
  return bareCatalogPrompt(options)
}
