/* A post's state chip: shipped, in progress or planned, next to the claim
   it qualifies. The dot colours follow the product's status tokens
   (tokens.css): review green for shipped, progress yellow, neutral for a
   plan. */
export type Status = `shipped` | `in-progress` | `planned`

const LABEL: Record<Status, string> = {
  shipped: `shipped`,
  "in-progress": `in progress`,
  planned: `planned`,
}

export function StatusChip({ status, children }: { status: Status; children?: string }) {
  return (
    <span className={`blog-status blog-status--${status}`}>
      <i aria-hidden />
      {children ?? LABEL[status]}
    </span>
  )
}
