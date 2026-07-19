/** Derive a short uppercase board prefix (e.g. "My Board" -> "MP") from a name. */
export function derivePrefix(name: string): string {
  // The server floor (boards.create, EXP-46) and the issue-ref token
  // contract (lib/issue-refs.ts) require a letter-led alphanumeric prefix —
  // drop symbol initials and leading digits so the derived value always
  // validates. Symbol/digit-only names derive `` and the create dialogs
  // require the user to type a prefix before submitting.
  return name
    .split(/[\s-_]+/)
    .map((w) => w[0] ?? ``)
    .join(``)
    .replace(/[^A-Za-z0-9]/g, ``)
    .replace(/^[0-9]+/, ``)
    .toUpperCase()
    .slice(0, 5)
}
