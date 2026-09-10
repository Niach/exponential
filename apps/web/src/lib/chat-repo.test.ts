import { describe, expect, it } from "vitest"
import {
  chatRepoOptions,
  chatStartInputs,
  defaultChatRepoId,
  NO_REPO,
} from "@/lib/chat-repo"

const repos = [
  { id: `repo-1`, fullName: `niach/exponential` },
  { id: `repo-2`, fullName: `niach/other` },
]

describe(`chat repository choice`, () => {
  it(`offers repo-less first, then every connected repo`, () => {
    expect(chatRepoOptions(repos)).toEqual([
      { value: NO_REPO, label: `No repository` },
      { value: `repo-1`, label: `niach/exponential` },
      { value: `repo-2`, label: `niach/other` },
    ])
  })

  it(`offers nothing when the team has no repo connected`, () => {
    // A one-entry menu reading "No repository" is noise on a page that is
    // meant to be one prompt box; the pages hide the picker on an empty list.
    expect(chatRepoOptions([])).toEqual([])
  })

  it(`pre-picks the only repo and otherwise stays repo-less`, () => {
    expect(defaultChatRepoId([repos[0]!])).toBe(`repo-1`)
    expect(defaultChatRepoId(repos)).toBe(``)
    expect(defaultChatRepoId([])).toBe(``)
  })

  it(`emits the repo input only when one is picked`, () => {
    // EXP-822: an ABSENT `repo` key is repo-less to the server; `repo: ""`
    // is a different thing and would be resolved as a missing repository.
    expect(chatStartInputs(`fix #EXP-1`, ``)).toEqual({ prompt: `fix #EXP-1` })
    expect(chatStartInputs(`fix #EXP-1`, `repo-1`)).toEqual({
      prompt: `fix #EXP-1`,
      repo: `repo-1`,
    })
  })
})
