// #EXP-2143 follow-up: "Confirm it's you" on the web (fixtures/step-up.json).
// `useStepUp` wraps an action behind Better Auth's fresh-session gate: when
// the action answers `not-fresh`, the sheet opens with the account's ways in;
// on proof (lib/auth/step-up.ts marks THIS session fresh) the action runs
// again by itself. The state machine is the shared reducer
// (`@exp/domain-contract/step-up`), replayed ×4.
import { useCallback, useRef, useState } from "react"
import { startAuthentication } from "@simplewebauthn/browser"
import {
  STEP_UP_CANCELLED,
  STEP_UP_COPY,
  STEP_UP_INITIAL,
  STEP_UP_NOT_FRESH,
  stepUpReduce,
  type StepUpEvent,
  type StepUpMethod,
  type StepUpState,
} from "@exp/domain-contract/step-up"

export { STEP_UP_COPY, STEP_UP_NOT_FRESH }
export type { StepUpMethod, StepUpState }

/** A gated action: `not-fresh` when the gate refused it, else it handled
 *  its own outcome (success or its own failure toast). */
export type GatedAction = () => Promise<`not-fresh` | void>

class StepUpRefused extends Error {
  constructor(readonly code: string) {
    super(code)
  }
}

async function authCall(path: string, body?: Record<string, unknown>): Promise<Record<string, unknown>> {
  const response = await fetch(`/api/auth${path}`, {
    method: body ? `POST` : `GET`,
    credentials: `include`,
    headers: body ? { "content-type": `application/json` } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  })
  let json: Record<string, unknown> = {}
  try {
    json = (await response.json()) as Record<string, unknown>
  } catch {
    // an empty or non-JSON answer: the status decides
  }
  if (!response.ok) throw new StepUpRefused(typeof json.code === `string` ? json.code : `HTTP_${response.status}`)
  return json
}

export type StepUp = ReturnType<typeof useStepUp>

export function useStepUp(onFailed: (message: string) => void) {
  const [state, setState] = useState<StepUpState>(STEP_UP_INITIAL)
  const [email, setEmail] = useState(``)
  const stateRef = useRef(state)
  const pending = useRef<GatedAction | null>(null)

  const dispatch = useCallback((event: StepUpEvent) => {
    stateRef.current = stepUpReduce(stateRef.current, event)
    setState(stateRef.current)
    return stateRef.current
  }, [])

  const sendCode = useCallback(async () => {
    try {
      await authCall(`/step-up/email/send`, {})
    } catch (err) {
      // The sheet stays; the error reads under the field.
      dispatch({ on: `submit` })
      dispatch({ on: `rejected`, code: err instanceof StepUpRefused ? err.code : `INTERNAL` })
    }
  }, [dispatch])

  const fail = useCallback(
    (next: StepUpState) => {
      onFailed(next.error ?? STEP_UP_COPY.notFresh)
      dispatch({ on: `done` })
    },
    [dispatch, onFailed]
  )

  /** Run a gated action; a refusal opens the sheet, a proof resumes it. */
  const run = useCallback(
    async (action: GatedAction) => {
      const result = await action()
      if (result !== `not-fresh`) {
        dispatch({ on: `done` })
        return
      }
      let methods: StepUpMethod[] = []
      if (!stateRef.current.retried) {
        try {
          const answer = await authCall(`/step-up/methods`)
          methods = (answer.methods as StepUpMethod[]) ?? []
          setEmail(typeof answer.email === `string` ? answer.email : ``)
        } catch {
          methods = []
        }
      }
      const next = dispatch({ on: `refused`, code: STEP_UP_NOT_FRESH, methods })
      if (next.action === `failed`) {
        pending.current = null
        fail(next)
        return
      }
      pending.current = action
      if (next.method === `email`) void sendCode()
    },
    [dispatch, fail, sendCode]
  )

  const resume = useCallback(async () => {
    const action = pending.current
    pending.current = null
    if (!action) return
    await run(action)
  }, [run])

  const choose = useCallback(
    (method: StepUpMethod) => {
      const before = stateRef.current.method
      const next = dispatch({ on: `choose`, method })
      if (next.method === `email` && before !== `email`) void sendCode()
    },
    [dispatch, sendCode]
  )

  /** Prove it: the passkey ceremony, the mailed code or the password. */
  const submit = useCallback(
    async (secret: string) => {
      const current = stateRef.current
      if (current.sheet !== `open` || !current.method) return
      dispatch({ on: `submit` })
      try {
        if (current.method === `passkey`) {
          const optionsJSON = await authCall(`/passkey/generate-authenticate-options`)
          let response
          try {
            response = await startAuthentication({ optionsJSON: optionsJSON as never })
          } catch {
            // NotAllowedError: the person dismissed the browser's prompt.
            throw new StepUpRefused(STEP_UP_CANCELLED)
          }
          await authCall(`/step-up/passkey`, { response: response as unknown as Record<string, unknown> })
        } else if (current.method === `email`) {
          await authCall(`/step-up/email/verify`, { code: secret.trim() })
        } else {
          await authCall(`/step-up/password`, { password: secret })
        }
      } catch (err) {
        dispatch({ on: `rejected`, code: err instanceof StepUpRefused ? err.code : `INTERNAL` })
        return
      }
      dispatch({ on: `proved` })
      await resume()
    },
    [dispatch, resume]
  )

  const cancel = useCallback(() => {
    pending.current = null
    dispatch({ on: `cancel` })
    dispatch({ on: `done` })
  }, [dispatch])

  return { state, email, run, choose, submit, cancel, resend: sendCode }
}
