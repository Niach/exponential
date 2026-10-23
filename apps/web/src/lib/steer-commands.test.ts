import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import {
  filterSteerCommands,
  matchSlashDraft,
  parseSteerCommand,
  steerAgentId,
  steerCommandDraft,
  steerCommandConfirmCopy,
  steerCommandsFor,
  COMPACTED_LABEL,
  COMPACTING_LABEL,
  CONFIG_DEFAULT_VALUE_LABEL,
  CONFIG_MODE_LABEL,
  DEFAULT_STEER_AGENT,
  EXTERNAL_STEER_AGENT,
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
    // NEW-2: the builtins are compact, new (thread 58cb32df), model and
    // effort, on both agents; `/clear` and `/btw` are gone.
    expect(claude).toEqual([`compact`, `new`, `model`, `effort`])
    expect(codex).toEqual([`compact`, `new`, `model`, `effort`])
    // EXP-849: a retired agent (historical rows still carry `pi`) gets the
    // empty catalog, like any unknown id.
    expect(steerCommandsFor(`pi`)).toEqual([])
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

// EXP-746: an external ACP agent syncs no agent at all, so "no agent" is not
// enough to mean claude. Mirrored ×4 with this exact name (iOS
// testAnAgentLessAcpRunIsAnExternalAgent, Android + desktop
// `an agent-less acp run is an external agent`).
describe(`steerAgentId`, () => {
  it(`an agent-less acp run is an external agent`, () => {
    // The sentinel is outside the contract on purpose: the curated rows
    // describe claude/codex behaviour, and an external agent's own
    // catalog is whatever it advertises.
    expect(contract.codingAgent.values).not.toContain(EXTERNAL_STEER_AGENT)
    expect(steerAgentId(null, true)).toBe(EXTERNAL_STEER_AGENT)
    expect(steerCommandsFor(steerAgentId(null, true))).toEqual([])
  })

  it(`a row that names its agent keeps it, ACP or not`, () => {
    expect(steerAgentId(`codex`, true)).toBe(`codex`)
    expect(steerAgentId(` codex `, true)).toBe(`codex`)
    expect(steerAgentId(`claude`, false)).toBe(`claude`)
  })

  it(`an agent-less run with no config_state is still a claude run`, () => {
    // A PTY run publishes no `config_state` — and neither does a row from a
    // desktop too old to stamp its agent.
    expect(steerAgentId(null, false)).toBe(DEFAULT_STEER_AGENT)
    expect(steerAgentId(``, false)).toBe(DEFAULT_STEER_AGENT)
    expect(steerAgentId(undefined, false)).toBe(DEFAULT_STEER_AGENT)
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
    expect(steerCommandDraft(byName(`model`))).toBe(`/model `)
    expect(steerCommandDraft({ ...byName(`compact`), argHint: `` })).toBe(`/compact`)
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
    // /clear, /btw, /init, /review are deliberately NOT in the catalog
    // (thread 58cb32df: `/new` is, `steerCommands` + the divider pill).
    expect(parseSteerCommand(`/new`, commands)?.command.name).toBe(`new`)
    for (const text of [`/clear`, `/btw why?`, `/init`, `/review`]) {
      expect(parseSteerCommand(text, commands)).toBeNull()
      expect(parseSteerCommand(text, steerCommandsFor(`codex`))).toBeNull()
    }
    expect(
      parseSteerCommand(`/model opus`, steerCommandsFor(`codex`))
    ).toMatchObject({ command: { name: `model` }, args: `opus` })
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

describe(`config chip copy (EXP-746)`, () => {
  it(`pins the chip labels byte for byte`, () => {
    expect(CONFIG_DEFAULT_VALUE_LABEL).toBe(`CLI default`)
    expect(CONFIG_MODE_LABEL).toBe(`Mode`)
  })
})
