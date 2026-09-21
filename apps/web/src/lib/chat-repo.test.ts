import { describe, expect, it } from "vitest"
import {
  chatRepoOptions,
  chatStartInputs,
  defaultChatRepoId,
} from "@/lib/chat-repo"

const repos = [
  { id: `repo-1`, fullName: `niach/exponential` },
  { id: `repo-2`, fullName: `niach/other` },
]

describe(`chat repository choice`, () => {
  it(`offers every connected repo and nothing else`, () => {
    // EXP-993: no "No repository" row — repo-less is not a choice any more.
    expect(chatRepoOptions(repos)).toEqual([
      { value: `repo-1`, label: `niach/exponential` },
      { value: `repo-2`, label: `niach/other` },
    ])
  })

  it(`offers nothing when the team has no repo connected`, () => {
    expect(chatRepoOptions([])).toEqual([])
  })

  it(`pre-picks the first repo`, () => {
    expect(defaultChatRepoId([repos[0]!])).toBe(`repo-1`)
    expect(defaultChatRepoId(repos)).toBe(`repo-1`)
    expect(defaultChatRepoId([])).toBe(``)
  })

  it(`emits the repo input only when one is picked`, () => {
    // EXP-822: an ABSENT `repo` key is repo-less to the server; `repo: ""`
    // is a different thing and would be resolved as a missing repository.
    // EXP-825: the text rides `prompt`, so repo-less means NO inputs.
    expect(chatStartInputs(``)).toBeUndefined()
    expect(chatStartInputs(`repo-1`)).toEqual({ repo: `repo-1` })
  })
})
