import { describe, expect, it } from "vitest"
import { writtenRoots } from "./client"

// #EXP-2021 (find-in-chat.json THE STORE CATCHING UP): the chats a write
// touched, so an open find bar of one of them asks the store again.
describe(`writtenRoots`, () => {
  it(`names the chat of a feed's events`, () => {
    expect([...writtenRoots(`putEvents`, [`r1`, [{ seq: 1 }]])]).toEqual([`r1`])
    expect(writtenRoots(`putEvents`, [`r1`, []]).size).toBe(0)
  })

  it(`names every chat of the synced messages`, () => {
    expect([...writtenRoots(`putMessages`, [[{ rootId: `r1` }, { rootId: `r2` }, { rootId: `r1` }]])].sort()).toEqual([`r1`, `r2`])
  })

  it(`a read touches nothing`, () => {
    expect(writtenRoots(`searchChat`, [`r1`, `wombat`, 50]).size).toBe(0)
  })
})
