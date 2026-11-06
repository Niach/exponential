// #VAPP-258: a host text link's chip (catalog/host.json `markdown.chipKinds`),
// on its own so `urls.ts` (and with it every primitive) checks a chip
// without pulling in the Markdown parser and the code tokenizer.

export interface MarkdownChip {
  kind: `issue` | `mention` | `entity`
  label: string
  icon?: string
  /** #VAPP-258: an identifier painted mono before the label (`Markdown/chipIdent`). */
  ident?: string
}

// Locked to catalog/host.json by markdown-host-constants.test.ts.
export const CHIP_KINDS: readonly string[] = [`issue`, `mention`, `entity`]

/** A host chip, kept only when its kind is known and it has a label. */
export function validChip(chip: unknown): MarkdownChip | undefined {
  if (!chip || typeof chip !== `object`) return undefined
  const c = chip as Record<string, unknown>
  if (typeof c.kind !== `string` || !CHIP_KINDS.includes(c.kind) || typeof c.label !== `string` || !c.label) return undefined
  return { kind: c.kind as MarkdownChip[`kind`], label: c.label, ...(typeof c.icon === `string` && c.icon ? { icon: c.icon } : {}), ...(typeof c.ident === `string` && c.ident ? { ident: c.ident } : {}) }
}
