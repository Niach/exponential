import { describe, expect, it } from "vitest"
import { projectTranscript, type SeqEvent } from "@/lib/steer-transcript-project"

// EXP-1216: the activity log → MCP message list projection.
const FIXTURE: SeqEvent[] = [
  { seq: 0, event: { kind: `user_message`, text: `Fix the bug`, at: 1000 } },
  { seq: 1, event: { kind: `config_state`, options: [] } },
  { seq: 2, event: { kind: `narration`, text: `Looking `, messageId: `m1`, at: 1001 } },
  // A subagent's rows never reach the top-level transcript, and (another
  // lane) never close the main bubble either.
  { seq: 3, event: { kind: `narration`, text: `sub prose`, messageId: `m1`, subagentId: `a1` } },
  {
    seq: 4,
    event: { kind: `tool`, name: `Read`, id: `t2`, toolKind: `read`, subagentId: `a1` },
  },
  { seq: 5, event: { kind: `user_message`, text: `sub turn`, subagentId: `a1` } },
  { seq: 6, event: { kind: `narration`, text: `at it.`, messageId: `m1`, at: 1002 } },
  {
    seq: 7,
    event: { kind: `tool`, name: `Bash`, detail: `bun test`, id: `t1`, toolKind: `execute` },
  },
  { seq: 8, event: { kind: `tool_update`, id: `t1`, status: `failed`, output: `boom` } },
  { seq: 9, event: { kind: `tool`, name: `Edit`, id: `t3`, toolKind: `edit` } },
  { seq: 10, event: { kind: `tool_update`, id: `t3`, status: `completed` } },
  // A tool row closed the bubble: the same id starts a new message.
  { seq: 11, event: { kind: `narration`, text: `More.`, messageId: `m1` } },
  {
    seq: 12,
    event: {
      kind: `question`,
      text: `Which env?`,
      id: `q1`,
      options: [
        { label: `prod`, key: `1` },
        { label: `staging`, key: `2` },
      ],
    },
  },
  { seq: 13, event: { kind: `usage`, contextUsed: 1, contextSize: 2 } },
  { seq: 14, event: { kind: `narration`, text: `No id.` } },
]

describe(`projectTranscript`, () => {
  it(`folds the fixture into top-level messages`, () => {
    expect(projectTranscript(FIXTURE)).toEqual([
      { seq: 0, at: 1000, role: `user`, text: `Fix the bug` },
      // Merged: the LAST fragment's seq, the first fragment's stamp.
      {
        seq: 6,
        at: 1001,
        role: `assistant`,
        text: `Looking at it.`,
        messageId: `m1`,
      },
      {
        seq: 7,
        role: `tool`,
        name: `Bash`,
        detail: `bun test`,
        kind: `execute`,
        failed: true,
      },
      { seq: 9, role: `tool`, name: `Edit`, kind: `edit` },
      { seq: 11, role: `assistant`, text: `More.`, messageId: `m1` },
      { seq: 12, role: `question`, text: `Which env?`, options: [`prod`, `staging`] },
      { seq: 14, role: `assistant`, text: `No id.` },
    ])
  })

  it(`marks a plan-approval card`, () => {
    expect(
      projectTranscript([
        {
          seq: 3,
          event: {
            kind: `question`,
            text: `# Plan`,
            id: `p1`,
            planMode: true,
            options: [{ label: `Approve`, key: `a` }],
          },
        },
      ])
    ).toEqual([
      { seq: 3, role: `question`, text: `# Plan`, options: [`Approve`], planMode: true },
    ])
  })

  it(`sorts by seq whatever the arrival order`, () => {
    const shuffled = [...FIXTURE].reverse()
    expect(projectTranscript(shuffled)).toEqual(projectTranscript(FIXTURE))
  })

  it(`returns nothing for an empty or state-only log`, () => {
    expect(projectTranscript([])).toEqual([])
    expect(
      projectTranscript([{ seq: 0, event: { kind: `turn`, state: `ended` } }])
    ).toEqual([])
  })
})
