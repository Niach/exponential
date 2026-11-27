import { describe, expect, it } from "vitest"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exp/domain-contract/fixtures/repo-ci.json"
import { secretName } from "@exp/domain-contract/repo-ci"
import {
  REPO_CI_EVENTS,
  REPO_CI_PATHS,
  SECRET_NAME_PATTERN,
  repoCiConfigAfter,
  repoCiSectionTree,
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
    // eCode: no GitHub switches, only the secrets for pull requests (#EXP-2180).
    expect(all(tree, `Switch`).map((s) => s.id)).toEqual([ids.pullRequestSecrets])
  })

  it(`no runs, no secrets: the muted texts`, () => {
    const tree = repoCiSectionTree(base({ pipelines: [], secrets: [] }))
    expect(find(tree, ids.pipelines)?.props).toMatchObject({ text: copy.ci.noPipeline, variant: `muted` })
    expect(find(tree, ids.secrets)?.props).toMatchObject({ text: copy.ci.secrets.empty, variant: `muted` })
  })

  it(`GitHub: the switches in order with the stored values`, () => {
    const tree = repoCiSectionTree(base({ provider: `github`, config: { githubStatuses: true, pullRequestSecrets: true } }))
    const switches = all(tree, `Switch`)
    expect(switches.map((s) => s.id)).toEqual([ids.githubStatuses, ids.actionsRunners, ids.pullRequestSecrets])
    expect(switches.map((s) => s.props?.checked)).toEqual([true, false, true])
    expect(find(tree, `${ids.pullRequestSecrets}-row`)?.props?.label).toBe(copy.ci.pullRequestSecrets.label)
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
    expect(repoCiConfigAfter(current, REPO_CI_EVENTS.pullRequestSecrets, { checked: true })).toEqual({ pullRequestSecrets: true })
    expect(repoCiConfigAfter(current, REPO_CI_EVENTS.pullRequestSecrets, { checked: false })).toBeNull()
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
