import { describe, expect, it } from "vitest"
import type { ToolItem } from "@/lib/steer-session-store"
import { commentCallTarget } from "./agent-comment-card"

// EXP-1241: the comment and issue a settled `exponential_comments_create`
// call's result preview names (refs `[comment, issue]`, wire doc §1b).

function call(extra: Partial<ToolItem>): ToolItem {
  return {
    id: 1,
    seq: 1,
    kind: `tool`,
    name: `mcp__exponential__exponential_comments_create`,
    settled: true,
    ...extra,
  } as ToolItem
}

describe(`commentCallTarget`, () => {
  it(`reads the comment id, its text and the issue off the preview refs`, () => {
    const target = commentCallTarget(
      call({
        detail: `EXP-9`,
        preview: {
          refs: [
            { kind: `comment`, id: `c-1`, title: `Pushed the fix.` },
            { kind: `issue`, id: `i-1`, identifier: `EXP-1253` },
          ],
        } as ToolItem[`preview`],
      })
    )
    expect(target).toEqual({
      commentId: `c-1`,
      commentText: `Pushed the fix.`,
      issueKey: `EXP-1253`,
    })
  })

  it(`without a preview the call's subject names the issue`, () => {
    expect(commentCallTarget(call({ detail: ` EXP-9 ` }))).toEqual({
      commentId: null,
      commentText: null,
      issueKey: `EXP-9`,
    })
  })
})
