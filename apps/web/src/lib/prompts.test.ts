import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/prompts.json"
import {
  deleteAccountPrompt,
  deleteActionPrompt,
  deleteFilePrompt,
  deleteIssuePrompt,
  deleteIssuesPrompt,
  deleteLabelPrompt,
  deleteTeamPrompt,
  deleteTriggerPrompt,
  fillPromptTemplate,
  leaveTeamPrompt,
  makeMemberPrompt,
  makeOwnerPrompt,
  mergeIssuePrPrompt,
  mergeExternalPrPrompt,
  mergeRunPrPrompt,
  moveIssuePrompt,
  PROMPT_FIXTURE,
  promptActions,
  removeDevicePrompt,
  removeMemberPrompt,
  removePasskeyPrompt,
  removePasswordPrompt,
  removeRepositoryPrompt,
  removeServerPrompt,
  resumeRunPrompt,
  stopRunPrompt,
  trashBoardPrompt,
  unlinkSignInMethodPrompt,
  WEB_PROMPTS,
  type PromptCopy,
} from "./prompts"

// EXP-1215: the web mirror of `prompts.json`, locked ×3 (iOS PromptsTests,
// Android PromptsTest). Every helper is rendered with its params set to
// `{name}` itself, so the output must equal the fixture byte for byte (a
// `{count}` body variant is rendered with count 2).

type Entry = {
  actions: { id: string; label: string; role: string }[]
  focus: string
  [key: string]: unknown
}

/** Each fixture text key → the helper call that must reproduce it. */
const CASES: Record<string, Record<string, () => PromptCopy>> = {
  "delete-issue": { title: () => deleteIssuePrompt(`{identifier}`) },
  "delete-issues": {
    titleOne: () => deleteIssuesPrompt(1),
    titleMany: () => deleteIssuesPrompt(`{count}` as unknown as number),
  },
  "delete-file": { title: () => deleteFilePrompt(`{filename}`) },
  "move-issue": { title: () => moveIssuePrompt(`{identifier}`, `{board}`) },
  "merge-issue-pr": {
    title: () =>
      mergeIssuePrPrompt({ number: `{number}` as unknown as number, count: 1 }),
    titleNoNumber: () => mergeIssuePrPrompt({ number: null, count: 1 }),
  },
  "merge-external-pr": {
    title: () =>
      mergeExternalPrPrompt(`{repository}`, `{number}` as unknown as number, `{base}`),
  },
  "merge-run-pr": {
    title: () => mergeRunPrPrompt(`{number}` as unknown as number),
    titleNoNumber: () => mergeRunPrPrompt(null),
  },
  "stop-run": { title: () => stopRunPrompt() },
  "resume-run": {
    title: () => resumeRunPrompt(`{device}`),
    titleNoDevice: () => resumeRunPrompt(null),
  },
  "delete-trigger": { title: () => deleteTriggerPrompt() },
  "delete-action": { title: () => deleteActionPrompt(`{name}`) },
  "remove-device": { title: () => removeDevicePrompt(`{name}`) },
  "delete-team": { title: () => deleteTeamPrompt(`{name}`) },
  "trash-board": { title: () => trashBoardPrompt(`{name}`) },
  "delete-label": { title: () => deleteLabelPrompt(`{name}`) },
  "remove-member": { title: () => removeMemberPrompt(`{name}`) },
  "leave-team": { title: () => leaveTeamPrompt(`{name}`) },
  "make-owner": { title: () => makeOwnerPrompt(`{name}`) },
  "make-member": { title: () => makeMemberPrompt(`{name}`) },
  "remove-repository": {
    title: () => removeRepositoryPrompt(`{fullName}`),
  },
  "unlink-sign-in-method": {
    title: () => unlinkSignInMethodPrompt(`{provider}`),
  },
  "remove-password": { title: () => removePasswordPrompt() },
  "remove-passkey": { title: () => removePasskeyPrompt(`{name}`) },
  "delete-account": {
    title: () => deleteAccountPrompt(),
    titleOnServer: () => deleteAccountPrompt(`{server}`),
  },
  "remove-server": { title: () => removeServerPrompt(`{server}`) },
}

const BODY_CASES: Record<string, Record<string, () => PromptCopy>> = {
  "delete-issues": {
    bodyOne: () => deleteIssuesPrompt(1),
    bodyMany: () => deleteIssuesPrompt(2),
  },
  "merge-issue-pr": {
    bodyOne: () => mergeIssuePrPrompt({ number: 1, count: 1 }),
    bodyMany: () => mergeIssuePrPrompt({ number: 1, count: 2 }),
  },
}

describe(`prompts (contract fixture)`, () => {
  it(`mirrors every entry the fixture lists, and nothing else`, () => {
    expect(Object.keys(CASES).sort()).toEqual(
      Object.keys(fixture.prompts).sort()
    )
    expect(PROMPT_FIXTURE).toBe(fixture.prompts)
  })

  for (const [id, raw] of Object.entries(fixture.prompts)) {
    const entry = raw as unknown as Entry
    describe(id, () => {
      it(`covers every text key`, () => {
        const textKeys = Object.keys(entry).filter((key) =>
          /^(title|body)/.test(key)
        )
        const covered = [
          ...Object.keys(CASES[id] ?? {}),
          ...Object.keys(BODY_CASES[id] ?? {}),
          ...(`body` in entry ? [`body`] : []),
        ]
        expect(covered.sort()).toEqual(textKeys.sort())
      })

      for (const [key, call] of Object.entries(CASES[id] ?? {})) {
        it(`${key} reproduces the fixture`, () => {
          const copy = call()
          expect(copy.title).toBe(entry[key])
          if (`body` in entry) {
            expect(copy.body).toBe(
              fillPromptTemplate(entry.body as string, {})
            )
          } else if (!(id in BODY_CASES)) {
            expect(copy.body).toBeUndefined()
          }
          // Labels, roles, display order, focus.
          expect(
            copy.actions.map(({ id: actionId, label, role }) => ({
              id: actionId,
              label,
              role,
            }))
          ).toEqual(entry.actions)
          expect(
            copy.actions.filter((action) => action.autoFocus).map((a) => a.id)
          ).toEqual([entry.focus])
        })
      }

      for (const [key, call] of Object.entries(BODY_CASES[id] ?? {})) {
        it(`${key} reproduces the fixture`, () => {
          expect(call().body).toBe(
            fillPromptTemplate(entry[key] as string, { count: 2 })
          )
        })
      }
    })
  }

  it(`fills params`, () => {
    expect(deleteFilePrompt(`report.pdf`).title).toBe(`Delete "report.pdf"?`)
    expect(mergeIssuePrPrompt({ number: 12, count: 3 })).toMatchObject({
      title: `Merge PR #12?`,
      body: `It is squash-merged. It covers 3 issues.`,
    })
    expect(deleteIssuesPrompt(4).title).toBe(`Delete 4 issues?`)
  })

  it(`never focuses a destructive answer`, () => {
    for (const make of Object.values(CASES).flatMap((c) => Object.values(c))) {
      for (const action of make().actions) {
        if (action.autoFocus) {
          expect([`destructive`, `quietDestructive`]).not.toContain(action.role)
        }
      }
    }
  })

  it(`attaches handlers by action id`, () => {
    const onSelect = () => {}
    const actions = promptActions(deleteLabelPrompt(`Bug`), {
      delete: { onSelect, busy: true },
    })
    expect(actions).toEqual([
      { label: `Cancel`, role: `cancel`, autoFocus: true },
      { label: `Delete`, role: `destructive`, onSelect, busy: true },
    ])
  })
})

describe(`web-only prompts`, () => {
  it(`follow the same rules: one question, focus never destructive`, () => {
    const copies = [
      WEB_PROMPTS.updateDevice(`Mac`),
      WEB_PROMPTS.sweepImages(2),
      WEB_PROMPTS.deleteWidget(`Site`),
      WEB_PROMPTS.deleteStatus(`QA`, 3),
      WEB_PROMPTS.cancelSubscription(`Team`, `May 1, 2027`),
      WEB_PROMPTS.removeMcpServer(`Linear`),
      WEB_PROMPTS.revokeApiKey(`CI`, `expu_abcd…`),
      WEB_PROMPTS.disconnectDeviceKey(`Mac`),
      WEB_PROMPTS.adminDeleteUser(`a@b.c`),
      WEB_PROMPTS.adminCompTier(`Acme`, `team`),
      WEB_PROMPTS.adminClearCompTier(`Acme`),
      WEB_PROMPTS.archiveBoard(`Web`),
    ]
    for (const copy of copies) {
      expect(copy.title.endsWith(`?`)).toBe(true)
      expect(copy.title).not.toMatch(/[“”—]/)
      expect(copy.body ?? ``).not.toMatch(/[“”—]|cannot be undone/)
      const focused = copy.actions.filter((action) => action.autoFocus)
      expect(focused).toHaveLength(1)
      expect([`destructive`, `quietDestructive`]).not.toContain(focused[0].role)
    }
  })

  it(`plural pairs and the Free plan numbers`, () => {
    expect(WEB_PROMPTS.sweepImages(1).title).toBe(
      `Delete 1 unreferenced image?`
    )
    expect(WEB_PROMPTS.deleteStatus(`QA`, 1).body).toBe(
      `1 issue moves to the status you pick.`
    )
    expect(WEB_PROMPTS.deleteStatus(`QA`, 0).body).toBeUndefined()
    expect(WEB_PROMPTS.cancelSubscription(`Team`, `May 1, 2027`).body).toBe(
      `The team keeps Team until May 1, 2027, then drops to Free: 3 seats, 250 MB, 1 widget. You can resume before then.`
    )
  })
})
