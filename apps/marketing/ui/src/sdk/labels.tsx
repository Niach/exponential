/* Small shared bits of the component pages: group names and inline `code`
   in catalog sentences. */
import { Fragment } from "react"

const GROUP_LABELS: Record<string, string> = {
  layout: `Layout`,
  list: `Lists`,
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

/** A catalog sentence with its `backticked` words as code. */
export function RichText({ text }: { text: string }) {
  const parts = text.split(/(`[^`]+`)/g)
  return (
    <>
      {parts.map((p, i) => (p.startsWith(`\``) && p.endsWith(`\``) && p.length > 2 ? <code key={i}>{p.slice(1, -1)}</code> : <Fragment key={i}>{p}</Fragment>))}
    </>
  )
}
