/* Small shared bits of the component pages: group names and the kind badges. */
import type { ComponentDoc } from "../lib/catalog"

const GROUP_LABELS: Record<string, string> = {
  layout: `Layout`,
  list: `Lists and rows`,
  text: `Text`,
  media: `Media`,
  navigation: `Navigation`,
  overlay: `Overlays`,
  feedback: `Feedback`,
  data: `Data`,
  action: `Actions`,
  input: `Inputs`,
  other: `Other`,
}

/** A group id → its heading (an unknown group shows capitalised). */
export const groupLabel = (group: string) => GROUP_LABELS[group] ?? group.charAt(0).toUpperCase() + group.slice(1)

export function KindBadges({ doc }: { doc: Pick<ComponentDoc, `kind` | `lite`> }) {
  return (
    <span className="sdk-badges">
      <span className={`sdk-badge is-${doc.kind}`} title={doc.kind === `native` ? `Painted by every renderer` : `Expands into natives before painting`}>
        {doc.kind}
      </span>
      {doc.lite && (
        <span className="sdk-badge is-lite" title="In the core-lite catalog (no overlays, media or Chart)">
          lite
        </span>
      )}
    </span>
  )
}
