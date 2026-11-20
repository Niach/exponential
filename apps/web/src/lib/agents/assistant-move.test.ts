import { afterEach, describe, expect, it, vi } from "vitest"

// #EXP-1986: the Assistant move runs only behind its switch, in ONE
// transaction under its lock, every statement in file order.
vi.mock(`@/db/connection`, () => ({ db: {} }))

const { assistantMoveOn, assistantMoveStatements, runAssistantMove } = await import(`./assistant-move`)

afterEach(() => {
  delete process.env.ASSISTANT_MOVE
})

function fakeDb() {
  const executed: string[] = []
  const tx = {
    execute: async (query: { queryChunks?: unknown[] }) => {
      executed.push(JSON.stringify(query))
    },
  }
  return { executed, db: { transaction: vi.fn(async (fn: (t: typeof tx) => Promise<void>) => fn(tx)) } }
}

describe(`assistant move switch (#EXP-1986)`, () => {
  it(`is off unless ASSISTANT_MOVE=on`, () => {
    expect(assistantMoveOn()).toBe(false)
    process.env.ASSISTANT_MOVE = `true`
    expect(assistantMoveOn()).toBe(false)
    process.env.ASSISTANT_MOVE = ` ON `
    expect(assistantMoveOn()).toBe(true)
  })

  it(`does nothing while off`, async () => {
    const fake = fakeDb()
    await runAssistantMove(fake.db as never)
    expect(fake.db.transaction).not.toHaveBeenCalled()
  })

  it(`on: one transaction, the lock first, then every statement in order, scratch tables dropped last`, async () => {
    process.env.ASSISTANT_MOVE = `on`
    const fake = fakeDb()
    await runAssistantMove(fake.db as never)
    expect(fake.db.transaction).toHaveBeenCalledTimes(1)
    const statements = assistantMoveStatements()
    expect(fake.executed).toHaveLength(statements.length + 1)
    expect(fake.executed[0]).toContain(`pg_advisory_xact_lock`)
    expect(statements[0]).toMatch(/CREATE TABLE "exp1986_movers"/)
    expect(statements.at(-1)).toMatch(/DROP TABLE "exp1986_movers"/)
  })
})
