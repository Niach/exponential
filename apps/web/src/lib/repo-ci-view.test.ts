import { describe, expect, it } from "vitest"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exp/domain-contract/fixtures/repo-ci.json"
import { runRows, secretName } from "@exp/domain-contract/repo-ci"
import {
  REPO_CI_EVENTS,
  REPO_CI_PATHS,
  SECRET_NAME_PATTERN,
  repoCiConfigAfter,
  repoCiSectionTree,
  repoRunInput,
  repoRunsTree,
  secretFromSubmit,
  type RepoCiSectionInput,
} from "./repo-ci-view"

function find(node: NestedNode, id: string): NestedNode | undefined {
  if (node.id === id) return node
  for (const child of [...(node.children ?? []), ...Object.values(node.slots ?? {})]) {
    const hit = find(child, id)
    if (hit) return hit
  }
  return undefined
}

function all(node: NestedNode, component: string): NestedNode[] {
  const own = node.component === component ? [node] : []
  return [...own, ...[...(node.children ?? []), ...Object.values(node.slots ?? {})].flatMap((c) => all(c, component))]
}

const ids = fixture.testIds
const copy = fixture.copy
const sample = fixture.runRows[0]!
const rows = runRows(sample.runs, sample.jobs, sample.now)

describe(`repoRunsTree (repo-ci.json RUNS TAB)`, () => {
  it(`wide: a Table with the fixture's columns in order, a status slot, rowPress opens the run`, () => {
    const tree = repoRunsTree(rows, false)
    expect(tree.id).toBe(ids.runs)
    expect(tree.component).toBe(`Table`)
    const columns = tree.props?.columns as Array<{ key: string; label: string; type?: string }>
    expect(columns.map((c) => c.label)).toEqual(Object.values(copy.runs.columns))
    expect(columns[0]).toMatchObject({ type: `slot`, slot: `status` })
    expect(columns[6]!.type).toBe(`relativeTime`)
    const tableRows = tree.props?.rows as Array<Record<string, unknown>>
    expect(tableRows.map((r) => r.id)).toEqual(rows.map((r) => r.id))
    expect(tableRows[1]).toMatchObject({ run: `#12 ci`, commit: `ccccccc`, ref: `feat/x`, trigger: `Pull request`, status: `Running`, duration: `5m 0s` })
    // Not started: no duration.
    expect(tableRows[0]!.duration).toBe(``)
    expect(tableRows.find((r) => r.id === `r11`)).toMatchObject({ glyph: `ci-failed`, iconTone: `danger`, ref: `—` })
    expect(tree.on?.rowPress?.event?.name).toBe(REPO_CI_EVENTS.openRun)
    expect(tree.slots?.status?.component).toBe(`Stack`)
  })

  it(`phones: one pressable Row per run (glyph, #n, name, subtitle, duration)`, () => {
    const tree = repoRunsTree(rows, true)
    const rs = all(tree, `Row`)
    expect(rs).toHaveLength(rows.length)
    expect(rs[1]!.id).toBe(`${ids.runRow}.r12`)
    expect(rs[1]!.props).toMatchObject({ identifier: `#12`, title: `ci`, subtitle: `feat/x · ccccccc · Pull request`, meta: `5m 0s`, pressable: true })
    expect(rs[0]!.props?.meta).toBeUndefined()
    expect(rs[1]!.slots?.leading?.props).toMatchObject({ name: `ci-running`, tone: `neutral` })
    expect(rs[1]!.on?.press?.event).toEqual({ name: REPO_CI_EVENTS.openRun, context: { run: `r12` } })
  })

  it(`repoRunInput turns synced Dates into ISO strings`, () => {
    const at = new Date(`2026-11-13T10:00:00.000Z`)
    expect(repoRunInput({ id: `a`, number: 1, name: `ci`, sha: `x`, trigger: `push`, state: `pending`, startedAt: at, completedAt: null })).toMatchObject({
      ref: null,
      startedAt: `2026-11-13T10:00:00.000Z`,
      completedAt: null,
    })
  })
})

const base = (over: Partial<RepoCiSectionInput> = {}): RepoCiSectionInput => ({
  phone: false,
  provider: `ecode`,
  manager: true,
  pipelines: [
    { name: `ci`, runId: `r13`, state: `pending` },
    { name: `nightly`, runId: `r11`, state: `failure` },
  ],
  suggestions: [`build`, `test`],
  secrets: [{ name: `NPM_TOKEN`, updatedAt: `2026-11-12T10:00:00.000Z` }],
  config: {},
  busy: false,
  ...over,
})

describe(`repoCiSectionTree (repo-ci.json SETTINGS TAB)`, () => {
  it(`a Section titled CI: pipelines with their Chip{dot}, required checks, secrets, then Add secret`, () => {
    const tree = repoCiSectionTree(base())
    expect(tree).toMatchObject({ id: ids.ci, component: `Section`, props: { title: copy.ci.section } })
    const pipes = find(tree, ids.pipelines)!
    expect(pipes.children?.map((r) => r.props?.title)).toEqual([`ci`, `nightly`])
    expect(pipes.children?.[1]?.slots?.trailing?.props).toMatchObject({ label: `Failing`, dot: `$color.destructive` })
    expect(pipes.children?.[0]?.slots?.trailing?.props).toMatchObject({ label: `Running`, dot: `$color.warning` })
    const chips = find(tree, ids.required)!
    expect(chips.component).toBe(`ChipInput`)
    expect(chips.props).toMatchObject({ values: { "@path": REPO_CI_PATHS.required }, placeholder: copy.ci.required.placeholder, disabled: false })
    expect(chips.props?.suggestions).toEqual([
      { label: `build`, value: `build` },
      { label: `test`, value: `test` },
    ])
    const secrets = find(tree, ids.secrets)!
    expect(secrets.component).toBe(`Table`)
    expect((secrets.props?.columns as Array<{ label: string }>).slice(0, 2).map((c) => c.label)).toEqual(Object.values(copy.ci.secrets.columns))
    expect(secrets.slots?.actions?.props?.label).toBe(copy.ci.secrets.delete)
    expect(find(tree, ids.addSecret)?.props?.label).toBe(copy.ci.secrets.add)
    expect(find(tree, `repo-ci-secret-dialog`)?.component).toBe(`Dialog`)
    expect(find(tree, `repo-ci-secret-value`)?.props?.type).toBe(`password`)
    expect(find(tree, `repo-ci-delete-secret`)?.props).toMatchObject({ title: copy.ci.secrets.deleteTitle, confirmLabel: copy.ci.secrets.deleteConfirm, destructive: true })
    // eCode: no GitHub switches.
    expect(all(tree, `Switch`)).toHaveLength(0)
  })

  it(`no runs, no secrets: the muted texts`, () => {
    const tree = repoCiSectionTree(base({ pipelines: [], secrets: [] }))
    expect(find(tree, ids.pipelines)?.props).toMatchObject({ text: copy.ci.noPipeline, variant: `muted` })
    expect(find(tree, ids.secrets)?.props).toMatchObject({ text: copy.ci.secrets.empty, variant: `muted` })
  })

  it(`GitHub: both switches in order with the stored values`, () => {
    const tree = repoCiSectionTree(base({ provider: `github`, config: { githubStatuses: true } }))
    const switches = all(tree, `Switch`)
    expect(switches.map((s) => s.id)).toEqual([ids.githubStatuses, ids.actionsRunners])
    expect(switches.map((s) => s.props?.checked)).toEqual([true, false])
    expect(find(tree, `${ids.githubStatuses}-row`)?.props?.label).toBe(copy.ci.githubStatuses.label)
  })

  it(`not a manager: every control disabled, no secret actions`, () => {
    const tree = repoCiSectionTree(base({ provider: `github`, manager: false }))
    expect(find(tree, ids.required)?.props?.disabled).toBe(true)
    expect(all(tree, `Switch`).every((s) => s.props?.disabled === true)).toBe(true)
    expect(find(tree, ids.addSecret)).toBeUndefined()
    expect(find(tree, `repo-ci-delete-secret`)).toBeUndefined()
    expect(find(tree, ids.secrets)?.slots?.actions).toBeUndefined()
  })

  it(`secrets still loading: no Secrets block yet`, () => {
    const tree = repoCiSectionTree(base({ secrets: null }))
    expect(find(tree, ids.secrets)).toBeUndefined()
    expect(find(tree, ids.addSecret)).toBeUndefined()
    expect(find(tree, ids.required)).toBeDefined()
  })

  it(`phones: secret Rows and a Drawer`, () => {
    const tree = repoCiSectionTree(base({ phone: true }))
    const row = find(tree, `${ids.secrets}.NPM_TOKEN`)!
    expect(row.component).toBe(`Row`)
    expect(row.slots?.trailing?.on?.press?.event).toEqual({ name: REPO_CI_EVENTS.deleteSecret, context: { name: `NPM_TOKEN` } })
    expect(find(tree, `repo-ci-secret-dialog`)?.component).toBe(`Drawer`)
  })
})

describe(`events`, () => {
  const current = { requiredChecks: [`build`], githubStatuses: false, actionsRunners: false }
  it(`required checks: trimmed, deduplicated, unchanged = nothing`, () => {
    expect(repoCiConfigAfter(current, REPO_CI_EVENTS.required, { values: [`build`, ` test `, `test`, ``] })).toEqual({ requiredChecks: [`build`, `test`] })
    expect(repoCiConfigAfter(current, REPO_CI_EVENTS.required, { values: [`build`] })).toBeNull()
  })
  it(`switches write their own key`, () => {
    expect(repoCiConfigAfter(current, REPO_CI_EVENTS.githubStatuses, { checked: true })).toEqual({ githubStatuses: true })
    expect(repoCiConfigAfter(current, REPO_CI_EVENTS.actionsRunners, { checked: false })).toBeNull()
  })
  it(`a secret submit passes secretName`, () => {
    expect(secretFromSubmit({ values: { name: `NPM_TOKEN`, value: `x` } })).toEqual({ name: `NPM_TOKEN`, value: `x` })
    expect(secretFromSubmit({ values: { name: `1BAD`, value: `x` } })).toBeNull()
    expect(secretFromSubmit({ values: { name: `OK`, value: `` } })).toBeNull()
  })
  it(`the Input's regex agrees with secretName`, () => {
    const re = new RegExp(SECRET_NAME_PATTERN)
    for (const name of [`A`, `_X`, `NPM_TOKEN`, `1A`, `a`, `A-B`, `A`.repeat(128), `A`.repeat(129)]) {
      expect(re.test(name)).toBe(secretName(name))
    }
  })
})
