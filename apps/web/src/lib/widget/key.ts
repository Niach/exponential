import { randomBytes } from "node:crypto"

// Public widget keys ship inside third-party pages' snippets, so they are
// identifiers, not secrets: stored in plaintext, gated by the domain
// allowlist + rate limiting. `expw_` mirrors the `expu_` personal-key prefix.
const widgetKeyAlphabet = `ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789`
const widgetKeyLength = 32

export const widgetKeyPattern = /^expw_[A-Za-z0-9]{32}$/

export function generateWidgetKey(): string {
  const bytes = randomBytes(widgetKeyLength)
  let suffix = ``
  for (const byte of bytes) {
    suffix += widgetKeyAlphabet[byte % widgetKeyAlphabet.length]
  }
  return `expw_${suffix}`
}

// Cheap pre-filter so malformed keys never reach the database.
export function isWidgetKeyFormat(value: string): boolean {
  return widgetKeyPattern.test(value)
}
