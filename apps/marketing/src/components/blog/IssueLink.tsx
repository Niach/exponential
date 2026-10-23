/* `#EXP-123` as a link into the tracker. The boards the posts cite live on
   the Exponential team (slug `feedback`); the prefix picks the board slug.
   An unknown prefix renders the identifier as plain text rather than a
   broken link. */
const TRACKER = `https://app.exponential.at/t/feedback/boards`

const BOARDS: Record<string, string> = {
  EXP: `exponential`,
  NEW: `new-exponential`,
  N: `next`,
  VAPP: `vapps`,
  FEED: `feedback`,
  PA: `potential-apps`,
}

export function IssueLink({ id }: { id: string }) {
  const prefix = id.split(`-`)[0]
  const board = BOARDS[prefix]
  if (!board) return <code>#{id}</code>
  return (
    <a className="blog-issue" href={`${TRACKER}/${board}/issues/${id}`} title={`Issue ${id} on the Exponential tracker`}>
      #{id}
    </a>
  )
}
