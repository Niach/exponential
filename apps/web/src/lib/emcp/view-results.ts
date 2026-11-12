// #VAPP-292 — the list ops that open an MCP Apps view (issues list/show,
// devices list, notifications list) answer TWO audiences outside a session:
// the model reads `content` and the view reads `structuredContent` (the MCP
// Apps spec: "structured data optimized for UI rendering, not added to model
// context"). A board of a few hundred issues as pretty JSON was ~100k chars,
// past Claude's tool-result cap, so the view only ever saw the cap's error.
// The model now gets a summary: a count, one line per row with the id it
// needs for a follow-up call, and the next page's offset; the view gets every
// row plus the page. Inside a launched session (no views) nothing changes.

type Content = { type: string; text?: string }
export type ViewToolResult = { content: unknown[]; isError?: boolean; structuredContent?: unknown }

/** noun → its ops whose answer is a view's list. */
export const VIEW_LIST_OPS: Readonly<Record<string, readonly string[]>> = {
  issues: [`list`, `show`],
  devices: [`list`],
  notifications: [`list`],
}

export function isViewListOp(noun: string, op: string): boolean {
  return VIEW_LIST_OPS[noun]?.includes(op) ?? false
}

/** The page a view asks for next (null = the last page). */
export interface ViewPage {
  offset: number
  limit: number
  nextOffset: number | null
}

const str = (value: unknown): string | null => (typeof value === `string` && value.length > 0 ? value : null)

/** A title cut for one line. */
function clip(text: string, max = 90): string {
  const flat = text.replace(/\s+/g, ` `).trim()
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat
}

function issueLine(row: Record<string, unknown>): string {
  const parts = [
    str(row.identifier) ?? str(row.id) ?? `?`,
    str(row.status),
    str(row.priority) && row.priority !== `none` ? String(row.priority) : null,
    clip(str(row.title) ?? ``),
  ]
  const tail = [
    row.assigneeId ? `assignee ${String(row.assigneeId)}` : null,
    str(row.dueDate) ? `due ${String(row.dueDate)}` : null,
    typeof row.prNumber === `number` ? `PR #${row.prNumber}` : null,
  ].filter(Boolean)
  return `- ${parts.filter(Boolean).join(` · `)}${tail.length ? ` (${tail.join(`, `)})` : ``}`
}

function deviceLine(row: Record<string, unknown>): string {
  const agents = Array.isArray(row.agents) ? (row.agents as unknown[]).map(String) : []
  const owner = row.owner && typeof row.owner === `object` ? str((row.owner as { name?: unknown }).name) : null
  const parts = [
    str(row.label) ?? str(row.deviceId) ?? `?`,
    row.online ? `online` : `offline`,
    str(row.platform),
    agents.length ? `agents ${agents.join(`, `)}` : `no agent`,
    owner ? `shared by ${owner}` : null,
  ]
  return `- ${parts.filter(Boolean).join(` · `)} (deviceId ${String(row.deviceId)})`
}

function notificationLine(row: Record<string, unknown>): string {
  const parts = [str(row.type), clip(str(row.title) ?? ``), row.readAt ? `read` : `unread`, str(row.createdAt)]
  return `- ${parts.filter(Boolean).join(` · `)} (id ${String(row.id)})`
}

const LINE: Record<string, (row: Record<string, unknown>) => string> = {
  issues: issueLine,
  devices: deviceLine,
  notifications: notificationLine,
}

const NOUN_LABEL: Record<string, [string, string]> = {
  issues: [`issue`, `issues`],
  devices: [`device`, `devices`],
  notifications: [`notification`, `notifications`],
}

/** The model's summary of one page. */
export function viewListSummary(noun: string, rows: readonly Record<string, unknown>[], page: ViewPage): string {
  const [one, many] = NOUN_LABEL[noun] ?? [`row`, `rows`]
  const first = rows.length ? page.offset + 1 : 0
  const head =
    rows.length === 0
      ? `No ${many}${page.offset ? ` past offset ${page.offset}` : ``}.`
      : `${rows.length} ${rows.length === 1 ? one : many} (${first}–${page.offset + rows.length}), shown to the person in the Exponential view.`
  const more =
    page.nextOffset === null ? `` : `\nMore: call again with offset ${page.nextOffset} (limit ${page.limit}).`
  const lines = rows.map(LINE[noun] ?? ((row) => `- ${JSON.stringify(row)}`))
  const detail = noun === `issues` && rows.length ? `\nDetails: exponential_issues {op: "get", id}.` : ``
  return [head, ...lines].join(`\n`) + more + detail
}

/** A view list op's answer split for the model and the view. Anything that
 *  is not a JSON array of rows (an error, a refusal) passes through. */
export function compactViewResult(
  noun: string,
  args: Record<string, unknown>,
  result: ViewToolResult
): ViewToolResult {
  if (result.isError) return result
  const text = (result.content as Content[] | undefined)?.find((block) => block.type === `text`)?.text
  if (typeof text !== `string`) return result
  let rows: unknown
  try {
    rows = JSON.parse(text)
  } catch {
    return result
  }
  if (!Array.isArray(rows)) return result
  const items = rows.filter((row): row is Record<string, unknown> => Boolean(row) && typeof row === `object`)
  const offset = typeof args.offset === `number` ? args.offset : 0
  const limit = typeof args.limit === `number` ? args.limit : 50
  const page: ViewPage = { offset, limit, nextOffset: items.length >= limit ? offset + items.length : null }
  return {
    content: [{ type: `text`, text: viewListSummary(noun, items, page) }],
    structuredContent: { items, page },
  }
}
