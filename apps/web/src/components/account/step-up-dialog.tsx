import { useEffect, useState } from "react"
import {
  Button,
  Dialog,
  DialogBody,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
} from "@exp/ui"
import { STEP_UP_COPY, type StepUp } from "@/lib/step-up"

// #EXP-2143 follow-up: THE "Confirm it's you" sheet (fixtures/step-up.json
// layout): title, the method's one line, its field, the inline error, the
// other methods as link buttons, then Cancel + the one primary button. It
// sits over whatever form refused (that form's draft is untouched).
export function StepUpDialog({ stepUp }: { stepUp: StepUp }) {
  const { state, email } = stepUp
  const [secret, setSecret] = useState(``)
  const open = state.sheet !== `closed`
  const busy = state.sheet === `busy`
  const method = state.method

  // A new method (or a new sheet) starts with an empty field.
  useEffect(() => setSecret(``), [method, open])

  if (!method) return null
  const description = STEP_UP_COPY.description[method].replace(`{email}`, email)
  const others = state.methods.filter((m) => m !== method)
  const submit = () => {
    if (busy) return
    if (method !== `passkey` && !secret.trim()) return
    void stepUp.submit(secret)
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) stepUp.cancel()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{STEP_UP_COPY.title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogBody className="space-y-2">
          {method === `passkey` ? null : (
            <>
              <Label htmlFor="step-up-secret">{STEP_UP_COPY.field[method]}</Label>
              <Input
                id="step-up-secret"
                autoFocus
                type={method === `password` ? `password` : `text`}
                inputMode={method === `email` ? `numeric` : undefined}
                autoComplete={method === `email` ? `one-time-code` : `current-password`}
                maxLength={method === `email` ? 6 : 512}
                value={secret}
                aria-invalid={state.error ? true : undefined}
                onChange={(e) => setSecret(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === `Enter`) submit()
                }}
              />
            </>
          )}
          {state.error ? <p className="text-sm text-destructive">{state.error}</p> : null}
          <div className="flex flex-wrap gap-x-4">
            {method === `email` ? (
              <Button variant="link" size="sm" className="px-0" disabled={busy} onClick={() => void stepUp.resend()}>
                {STEP_UP_COPY.resend}
              </Button>
            ) : null}
            {others.map((other) => (
              <Button
                key={other}
                variant="link"
                size="sm"
                className="px-0"
                disabled={busy}
                onClick={() => stepUp.choose(other)}
              >
                {STEP_UP_COPY.switch[other]}
              </Button>
            ))}
          </div>
        </DialogBody>
        <DialogFooter>
          <DialogCancel variant="outline" onClick={() => stepUp.cancel()} />
          <Button onClick={submit} pending={busy}>
            {method === `passkey` ? STEP_UP_COPY.passkey : STEP_UP_COPY.confirm}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
