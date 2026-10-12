import { describe, expect, it } from "vitest"
import { TRPCClientError } from "@trpc/client"
import { FEEDBACK_STATES } from "@exp/domain-contract/feedback-mark"
import {
  draftStartKey,
  sameDraftStart,
  startMaybeDelivered,
  type DraftStartFields,
} from "@/lib/start-intent"

const FINGERPRINT = FEEDBACK_STATES.starting.fingerprint

// Recheck R2: the fixture's cases, replayed verbatim.
describe(`sameDraftStart (feedback-states.json starting.fingerprint)`, () => {
  for (const testCase of FINGERPRINT.cases) {
    it(testCase.name, () => {
      expect(
        sameDraftStart(
          testCase.a as DraftStartFields,
          testCase.b as DraftStartFields
        )
      ).toBe(testCase.same)
    })
  }

  it(`keys every listed field`, () => {
    const base = FINGERPRINT.cases[0]!.a as DraftStartFields
    const changes: Record<string, Partial<DraftStartFields>> = {
      deviceId: { deviceId: `d9` },
      text: { text: `x` },
      "issueIds (set)": { issueIds: [`i9`] },
      actionId: { actionId: `act9` },
      "actionInputs (key/value map)": { actionInputs: { repo: `r9` } },
      "repositoryId (chat)": { repositoryId: `r9` },
      "attachments (set of local pick ids)": { attachments: [] },
      agent: { agent: `codex` },
      model: { model: `haiku` },
      effort: { effort: `` },
      planMode: { planMode: true },
      account: { account: null },
      "mcpServerIds (set)": { mcpServerIds: [] },
      computerUse: { computerUse: false },
      ultracode: { ultracode: true },
      subagentModel: { subagentModel: `haiku` },
      "resume (the composer's Resume toggle; absent = false)": { resume: true },
    }
    expect(Object.keys(changes).sort()).toEqual([...FINGERPRINT.fields].sort())
    for (const change of Object.values(changes)) {
      expect(sameDraftStart(base, { ...base, ...change })).toBe(false)
    }
  })

  it(`absent and null are equal; action inputs compare as a map`, () => {
    const bare: DraftStartFields = { deviceId: `d1`, text: `hi` }
    expect(
      draftStartKey({
        ...bare,
        issueIds: null,
        actionId: null,
        actionInputs: {},
        mcpServerIds: [],
        account: undefined,
      })
    ).toBe(draftStartKey(bare))
    expect(
      sameDraftStart(
        { ...bare, actionId: `a`, actionInputs: { x: `1`, y: `2` } },
        { ...bare, actionId: `a`, actionInputs: { y: `2`, x: `1` } }
      )
    ).toBe(true)
  })
})

// A server answer: tRPC fills `data` with the code and HTTP status.
function serverError(code: string, httpStatus: number): TRPCClientError<never> {
  const error = new TRPCClientError(`Steer relay error (${httpStatus})`)
  ;(error as { data?: unknown }).data = { code, httpStatus }
  return error as TRPCClientError<never>
}

// Recheck R3: `starting.maybeDelivered`.
describe(`startMaybeDelivered`, () => {
  it(`an unknown fate may have reached the device`, () => {
    const transport = new TRPCClientError(`Failed to fetch`, {
      cause: new TypeError(`Failed to fetch`),
    })
    expect(startMaybeDelivered(transport)).toBe(true)
    const timeout = new TRPCClientError(`The operation timed out`, {
      cause: Object.assign(new Error(`timed out`), { name: `TimeoutError` }),
    })
    expect(startMaybeDelivered(timeout)).toBe(true)
    expect(startMaybeDelivered(new TypeError(`Load failed`))).toBe(true)
    // The server's relay_timeout answer.
    expect(startMaybeDelivered(serverError(`GATEWAY_TIMEOUT`, 504))).toBe(true)
  })

  it(`a definite error never did`, () => {
    expect(startMaybeDelivered(serverError(`PRECONDITION_FAILED`, 412))).toBe(false)
    expect(startMaybeDelivered(serverError(`BAD_REQUEST`, 400))).toBe(false)
    expect(startMaybeDelivered(serverError(`INTERNAL_SERVER_ERROR`, 500))).toBe(false)
    expect(startMaybeDelivered(serverError(`BAD_GATEWAY`, 502))).toBe(false)
  })
})
