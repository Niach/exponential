import Foundation

/// EXP-948 — our OWN tools never hide. An Exponential MCP call is the most
/// meaningful row a transcript has — "Read issue EXP-901", "Opened pull
/// request" — and until now a pile of them vanished inside a collapsed
/// "Ran 1 command · 21 other tools" fold. This type owns the ONE rule that
/// keeps them visible and the ONE caption a run of them renders, over the
/// contract's `expToolDisplay` table (`ExpToolDisplay` does the name lookup,
/// so nothing here re-implements it).
///
/// Hand-mirrored ×4 (TS `@exp/domain-contract` `exp-tool-group.ts` + web
/// `lib/agent-feed.ts` `scanToolRuns`, desktop `steer::exp_tool_group`,
/// Android `domain/ExpToolGroup.kt`) and byte-locked by
/// `packages/domain-contract/fixtures/feed/exp-tool-groups.json`, which every
/// client's feed test replays through its own feed-row projection
/// (`AgentFeed.rows` / `AgentFeed.laneRows` here).
///
/// Grouping (a render ROW of the feed projection, `AgentFeedRow.expRun`):
/// - an Exponential call is a `tool` item that is NOT a workflow card (EXP-850
///   §3; on iOS a workflow call is named by its `callId` in the projection's
///   `workflowIds`, there is no field) whose NAME resolves to an
///   `expToolDisplay` row — the contract prefix right in front of a known row
///   name, whatever namespace an adapter put in front of THAT;
/// - an Exponential call NEVER joins a generic tool run — it breaks one exactly
///   like an edit call does (EXP-916), so it is never counted as "N other
///   tools";
/// - an `expRun` row is a MAXIMAL run of ≥2 consecutive items of ONE lane
///   (`subagentId`, nil = the main lane) that are Exponential calls of the SAME
///   contract row. A single call stays the single visible row it always was,
///   and two DIFFERENT Exponential tools in sequence are two rows;
/// - the rule reads ONLY the kind, the name, the workflow id and the lane —
///   never `settled`/`failed` — so a group never re-splits when a call settles;
/// - the row's id is its FIRST item's id (stable while a live run grows).
///
/// The caption (`ExpToolGroup.caption`, TS `expToolGroupCaption`):
/// - the contract's `progressiveMany` while ANY member is still running, its
///   `doneMany` once every member settled, `{n}` = the member count
///   ("Reading 3 issues" → "Read 3 issues");
/// - a failed member appends ` · N failed`, the same tail (and the same
///   separator) `ToolGroupSummary` writes.
public enum ExpToolGroup {
    /// The `expToolDisplay` row index a tool NAME belongs to, or nil for any
    /// other server's tool. The lookup itself is `ExpToolDisplay.resolve` — the
    /// prefix rule lives there and is mirrored nowhere else.
    public static func index(ofTool name: String) -> Int? {
        guard let display = ExpToolDisplay.resolve(toolName: name) else { return nil }
        return DomainContract.expToolNames.firstIndex(of: display.row)
    }

    /// The contract row NAME a feed item's call resolves to (`issues_get`), or
    /// nil when the item is not one of OUR calls.
    public static func rowName(_ item: AgentFeedItem) -> String? {
        guard case let .tool(_, name, _, _, _, _, _, _, _, _, _) = item else { return nil }
        return ExpToolDisplay.resolve(toolName: name)?.row
    }

    /// Whether a feed item is a call to one of OUR tools. `workflowIds` names
    /// the calls that ARE workflow cards (EXP-850) — such a call is its own row
    /// and never a group member.
    public static func isExpToolCall(
        _ item: AgentFeedItem, workflowIds: Set<String> = []
    ) -> Bool {
        guard case let .tool(_, name, _, _, callId, _, _, _, _, _, _) = item
        else { return false }
        if !workflowIds.isEmpty, let callId, workflowIds.contains(callId) { return false }
        return ExpToolDisplay.resolve(toolName: name) != nil
    }

    /// The inclusive end index of the maximal run of same-lane calls to the
    /// SAME Exponential tool that starts at `start` (which must itself be one).
    /// A caller's group scan uses it exactly like its edit-run scan.
    public static func runEnd(
        _ feed: [AgentFeedItem], start: Int, workflowIds: Set<String> = []
    ) -> Int {
        guard feed.indices.contains(start),
              isExpToolCall(feed[start], workflowIds: workflowIds),
              let row = rowName(feed[start])
        else { return start }
        let lane = feed[start].subagentKey
        var end = start
        while end + 1 < feed.count,
              isExpToolCall(feed[end + 1], workflowIds: workflowIds),
              feed[end + 1].subagentKey == lane,
              rowName(feed[end + 1]) == row {
            end += 1
        }
        return end
    }

    /// The caption an `expRun` row reads: the contract's plural copy for the
    /// run's tool, progressive while any member is still in flight, done once
    /// they all settled, plus ` · N failed` when members failed.
    public static func caption(_ items: [AgentFeedItem]) -> String {
        guard let first = items.first,
              case let .tool(_, name, _, _, _, _, _, _, _, _, _) = first,
              let index = index(ofTool: name),
              // The plural tables are generated from the SAME contract list, so
              // they are the same length — the guard is a belt against a
              // half-regenerated client, which must render an empty caption
              // rather than crash.
              DomainContract.expToolProgressiveMany.indices.contains(index),
              DomainContract.expToolDoneMany.indices.contains(index)
        else { return "" }
        let running = items.contains { item in
            guard case let .tool(_, _, _, _, _, _, settled, _, _, _, _) = item
            else { return true }
            return !settled
        }
        let template = running
            ? DomainContract.expToolProgressiveMany[index]
            : DomainContract.expToolDoneMany[index]
        // The FIRST `{n}` only — the TS source of truth is `String.replace`
        // with a string pattern, which never touches a second occurrence.
        let caption: String
        if let slot = template.range(of: "{n}") {
            caption = template.replacingCharacters(in: slot, with: String(items.count))
        } else {
            caption = template
        }
        let failed = items.filter { item in
            guard case let .tool(_, _, _, _, _, _, _, failed, _, _, _) = item else { return false }
            return failed
        }.count
        guard failed > 0 else { return caption }
        return "\(caption)\(ToolGroupSummary.separator)\(failed) failed"
    }
}
