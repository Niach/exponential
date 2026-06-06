// TypeScript entry point for the canonical design tokens. The native constants
// are emitted by scripts/generate.ts; this export lets the web side (and tests)
// read the same tokens.json. The web theme itself lives in
// apps/web/src/styles.css and remains the human-authored source for the OKLCH
// palette that tokens.json mirrors.

import tokensJson from "../tokens.json" with { type: "json" }

export const designTokens = tokensJson
export type DesignTokens = typeof tokensJson
