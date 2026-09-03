import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import {
  filterSteerCommands,
  matchSlashDraft,
  parseSteerCommand,
  steerCommandDraft,
  steerCommandConfirmCopy,
  steerCommandsFor,
  COMPACTED_LABEL,
  COMPACTING_LABEL,
  DEFAULT_STEER_AGENT,
  STEER_COMMANDS,
} from "./steer-commands"

// EXP-724 — the slash-command rules the four viewers implement identically.
// Every assertion here is mirrored by ExpCore's SlashCommandsTests,
// Android's SlashCommandsTest and the desktop's slash_commands tests; a
// change on one side without the others is a protocol drift, not a tweak.

describe(`steerCommandsFor`, () => {
  it(`filters the catalog by agent, in catalog order`, () => {
    const claude = steerCommandsFor(`claude`).map((c) => c.name)
    const codex = steerCommandsFor(`codex`).map((c) => c.name)
    expect(claude).toEqual([`compact`, `clear`])
    // Every agent sees the same two rows — the desktop maps `/clear` per
    // agent (pi runs it natively).
    expect(codex).toEqual(claude)
    expect(steerCommandsFor(`pi`).map((c) => c.name)).toEqual(claude)
    // Catalog order is preserved (a prefix of the full list's order).
    const order = STEER_COMMANDS.map((c) => c.name)
    expect(claude).toEqual(order.filter((n) => claude.includes(n)))
  })

  it(`an agent-less session is a claude run`, () => {
    expect(DEFAULT_STEER_AGENT).toBe(contract.codingAgent.values[0])
    expect(steerCommandsFor(null)).toEqual(steerCommandsFor(DEFAULT_STEER_AGENT))
    expect(steerCommandsFor(``)).toEqual(steerCommandsFor(DEFAULT_STEER_AGENT))
    expect(steerCommandsFor(undefined)).toEqual(
      steerCommandsFor(DEFAULT_STEER_AGENT)
    )
  })

  it(`an unknown agent offers nothing`, () => {
    expect(steerCommandsFor(`gizmo`)).toEqual([])
  })
})

describe(`matchSlashDraft`, () => {
  it(`opens only for a whole-draft command token`, () => {
    expect(matchSlashDraft(`/`)).toBe(``)
    expect(matchSlashDraft(`/co`)).toBe(`co`)
    expect(matchSlashDraft(`/fix-it`)).toBe(`fix-it`)
    expect(matchSlashDraft(`/CO`)).toBe(`CO`)
  })

  it(`never opens once whitespace, prose or another slash is in the draft`, () => {
    expect(matchSlashDraft(``)).toBeNull()
    expect(matchSlashDraft(`hi /co`)).toBeNull()
    expect(matchSlashDraft(`/compact now`)).toBeNull()
    expect(matchSlashDraft(` /compact`)).toBeNull()
    expect(matchSlashDraft(`/compact\n`)).toBeNull()
    expect(matchSlashDraft(`//`)).toBeNull()
    expect(matchSlashDraft(`/path/to/file`)).toBeNull()
  })
})

describe(`filterSteerCommands`, () => {
  const commands = steerCommandsFor(`claude`)

  it(`an empty query offers every command the agent has`, () => {
    expect(filterSteerCommands(commands, ``)).toEqual(commands)
  })

  it(`filters case-insensitively by name PREFIX, in catalog order`, () => {
    expect(filterSteerCommands(commands, `co`).map((c) => c.name)).toEqual([
      `compact`,
    ])
    expect(filterSteerCommands(commands, `CO`).map((c) => c.name)).toEqual([
      `compact`,
    ])
    // A substring is not a prefix.
    expect(filterSteerCommands(commands, `pact`)).toEqual([])
    expect(filterSteerCommands(commands, `zzz`)).toEqual([])
  })

  it(`honors the limit`, () => {
    expect(filterSteerCommands(commands, ``, 2)).toHaveLength(2)
  })
})

describe(`steerCommandDraft`, () => {
  const commands = steerCommandsFor(`claude`)
  const byName = (name: string) => commands.find((c) => c.name === name)!

  it(`leaves a trailing space only for a command that takes an argument`, () => {
    expect(steerCommandDraft(byName(`compact`))).toBe(`/compact `)
    expect(steerCommandDraft(byName(`clear`))).toBe(`/clear`)
  })
})

describe(`parseSteerCommand`, () => {
  const commands = steerCommandsFor(`claude`)

  it(`matches on the first whitespace token, case-insensitively`, () => {
    expect(parseSteerCommand(`/compact`, commands)).toMatchObject({
      command: { name: `compact` },
      args: ``,
    })
    expect(parseSteerCommand(`/COMPACT`, commands)?.command.name).toBe(`compact`)
    expect(parseSteerCommand(`/compact keep the plan`, commands)).toMatchObject({
      command: { name: `compact` },
      args: `keep the plan`,
    })
    // Leading/trailing whitespace around the whole message is ignored.
    expect(parseSteerCommand(`  /compact  `, commands)?.args).toBe(``)
  })

  it(`is not fooled by prose, a prefix or another agent's command`, () => {
    expect(parseSteerCommand(`please /compact now`, commands)).toBeNull()
    expect(parseSteerCommand(`/compactify`, commands)).toBeNull()
    expect(parseSteerCommand(`compact`, commands)).toBeNull()
    expect(parseSteerCommand(``, commands)).toBeNull()
    // /new, /model, /init, /review are deliberately NOT in the catalog.
    for (const text of [`/new`, `/model opus`, `/init`, `/review`]) {
      expect(parseSteerCommand(text, commands)).toBeNull()
      expect(parseSteerCommand(text, steerCommandsFor(`codex`))).toBeNull()
    }
    expect(
      parseSteerCommand(`/clear`, steerCommandsFor(`codex`))?.command.name
    ).toBe(`clear`)
  })
})

describe(`copy`, () => {
  it(`pins the compaction labels byte for byte`, () => {
    // U+2026, not three dots — the natives assert the same literals.
    expect(COMPACTING_LABEL).toBe(`Compacting context…`)
    expect(COMPACTING_LABEL).toContain(`…`)
    expect(COMPACTED_LABEL).toBe(`Context compacted`)
  })

  it(`pins the confirmation copy`, () => {
    expect(steerCommandConfirmCopy(`clear`)).toEqual({
      title: `Run /clear?`,
      body: `The agent forgets everything in this session so far. Files in the worktree are kept.`,
      confirm: `Run /clear`,
      cancel: `Cancel`,
    })
    expect(steerCommandConfirmCopy(`compact`).title).toBe(`Run /compact?`)
  })
})
