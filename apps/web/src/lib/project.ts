/** Derive a short uppercase project prefix (e.g. "My Project" -> "MP") from a name. */
export function derivePrefix(name: string): string {
  return name
    .split(/[\s-_]+/)
    .map((w) => w[0] ?? ``)
    .join(``)
    .toUpperCase()
    .slice(0, 5)
}
