// EXP-948: our OWN tools never hide. An Exponential MCP call is the most
// meaningful row a transcript has — "Read issue EXP-901", "Opened pull
// request" — and until now a pile of them vanished inside a collapsed
// "Ran 1 command · 21 other tools" fold. This module owns the ONE rule that
// keeps them visible and the ONE caption a run of them renders, over the
// contract's `expToolDisplay` table.
//
// Hand-mirrored ×4 (web `lib/agent-feed.ts` `scanToolRuns`, desktop
// `steer::exp_tool_group`, iOS `ExpCore/Sources/Domain/ExpToolGroup.swift`,
// Android `domain/ExpToolGroup.kt`) and byte-locked by
// `fixtures/feed/exp-tool-groups.json`, which every client's feed test replays
// through its own feed-row projection.
//
// Grouping (a render ROW of the feed projection, kind `expRun`):
// - an Exponential call is a `tool` item with no `workflowId` whose NAME
//   resolves to an `expToolDisplay` row (`expToolRowName`: the contract prefix
//   right in front of a known row name, whatever namespace an adapter put in
//   front of THAT);
// - an Exponential call NEVER joins a generic tool run — it breaks one exactly
//   like an edit call does (EXP-916), so it is never counted as "N other
//   tools";
// - an `expRun` row is a MAXIMAL run of ≥2 consecutive items of ONE lane
//   (`subagentId`, undefined = the main lane) that are Exponential calls of the
//   SAME contract row. A single call stays the single visible row it always
//   was, and two DIFFERENT Exponential tools in sequence are two rows;
// - the rule reads ONLY `kind`/`name`/`workflowId`/`subagentId` — never
//   `settled`/`failed` — so a group never re-splits when a call settles;
// - the row's id is its FIRST item's id (stable while a live run grows).
//
// The caption (`expToolGroupCaption`):
// - the contract's `progressiveMany` while ANY member is still running, its
//   `doneMany` once every member settled, `{n}` = the member count
//   ("Reading 3 issues" → "Read 3 issues");
// - a failed member appends ` · N failed`, the same tail (and the same
//   separator) `toolGroupSummary` writes.

import contractJson from "../contract.json" with { type: "json" }
import { TOOL_GROUP_SUMMARY_SEPARATOR } from "./tool-group-summary"

const json = contractJson as unknown as {
  expToolDisplay: {
    prefix: string
    tools: {
      name: string
      progressiveMany: string
      doneMany: string
    }[]
  }
}

/** The wire prefix every Exponential MCP tool carries (`exponential_`). */
export const EXP_TOOL_PREFIX: string = json.expToolDisplay.prefix

/** A feed item, narrowed to what the rule and the caption read. */
export interface ExpToolFeedItem {
  id: number
  kind: string
  /** The tool's wire name, whatever namespace the adapter put in front. */
  name?: string
  workflowId?: string
  subagentId?: string
  settled?: boolean
  failed?: boolean
}

/** The `expToolDisplay` row index a tool NAME belongs to, or `null` for any
 *  other server's tool. The contract PREFIX is REQUIRED right in front of the
 *  row name, which is what keeps another MCP server's `issues_create` out. */
export function expToolIndex(name: string | null | undefined): number | null {
  if (!name) return null
  const trimmed = name.trim()
  const tools = json.expToolDisplay.tools
  for (let i = 0; i < tools.length; i++) {
    const row = tools[i].name
    if (trimmed.length <= row.length) continue
    if (!trimmed.endsWith(row)) continue
    if (trimmed.slice(0, trimmed.length - row.length).endsWith(EXP_TOOL_PREFIX)) {
      return i
    }
  }
  return null
}

/** The contract row NAME a tool call resolves to (`issues_get`), or null. */
export function expToolRowName(name: string | null | undefined): string | null {
  const index = expToolIndex(name)
  return index === null ? null : json.expToolDisplay.tools[index].name
}

/** Whether a feed item is a call to one of OUR tools. */
export function isExpToolCall(
  item: Pick<ExpToolFeedItem, `kind` | `name` | `workflowId`>
): boolean {
  return (
    item.kind === `tool` &&
    item.workflowId === undefined &&
    expToolIndex(item.name) !== null
  )
}

/**
 * The inclusive end index of the maximal run of same-lane calls to the SAME
 * Exponential tool that starts at `start` (which must itself be one). A
 * caller's group scan uses it exactly like its edit-run scan.
 */
export function expToolRunEnd<
  T extends Pick<ExpToolFeedItem, `kind` | `name` | `workflowId` | `subagentId`>,
>(feed: readonly T[], start: number): number {
  const lane = feed[start]?.subagentId
  const row = expToolRowName(feed[start]?.name)
  if (row === null) return start
  let end = start
  while (
    end + 1 < feed.length &&
    isExpToolCall(feed[end + 1]) &&
    feed[end + 1].subagentId === lane &&
    expToolRowName(feed[end + 1].name) === row
  ) {
    end++
  }
  return end
}

/**
 * The caption an `expRun` row reads: the contract's plural copy for the run's
 * tool, progressive while any member is still in flight, done once they all
 * settled, plus ` · N failed` when members failed.
 */
export function expToolGroupCaption(
  items: readonly Pick<ExpToolFeedItem, `name` | `settled` | `failed`>[]
): string {
  const index = expToolIndex(items[0]?.name)
  if (index === null) return ``
  const row = json.expToolDisplay.tools[index]
  const running = items.some((item) => item.settled !== true)
  const caption = (running ? row.progressiveMany : row.doneMany).replace(
    `{n}`,
    String(items.length)
  )
  const failed = items.filter((item) => item.failed === true).length
  return failed === 0
    ? caption
    : `${caption}${TOOL_GROUP_SUMMARY_SEPARATOR}${failed} failed`
}
