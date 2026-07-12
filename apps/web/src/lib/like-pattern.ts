// Escape LIKE/ILIKE metacharacters (%, _, \) so user input matches literally
// when interpolated into a pattern (Postgres' default escape char is `\`).
export function escapeLikePattern(input: string): string {
  return input.replace(/[%_\\]/g, (m) => `\\${m}`)
}
