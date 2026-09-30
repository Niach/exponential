import { useState } from "react"
import { authClient, invalidateSessionCache } from "@/lib/auth/client"
import { authErrorMessage } from "@/lib/auth/error-messages"
import { useSession } from "@/hooks/use-session"
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

// EXP-1126: change the primary email. Two steps against Better Auth's
// email-otp plugin: the new address gets a 6-digit code
// (`/email-otp/request-email-change`), the code swaps the address and marks
// it verified (`/email-otp/change-email`). The current mailbox is not
// re-proven; the signed-in session is the ownership proof. Mirrors the login
// page's code step so the two feel like one product.
export function ChangeEmailDialog({
  open,
  currentEmail,
  onOpenChange,
  onChanged,
}: {
  open: boolean
  currentEmail: string
  onOpenChange: (open: boolean) => void
  onChanged: (newEmail: string) => void
}) {
  const { refetch } = useSession()
  const [step, setStep] = useState<`address` | `code`>(`address`)
  const [newEmail, setNewEmail] = useState(``)
  const [code, setCode] = useState(``)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(``)

  const reset = () => {
    setStep(`address`)
    setNewEmail(``)
    setCode(``)
    setBusy(false)
    setError(``)
  }

  const close = () => {
    onOpenChange(false)
    reset()
  }

  const sendCode = async (address: string) => {
    const target = address.trim().toLowerCase()
    if (!target) return
    if (target === currentEmail.toLowerCase()) {
      setError(`That is already your email.`)
      return
    }
    setBusy(true)
    setError(``)
    try {
      const { error: sendError } = await authClient.emailOtp.requestEmailChange({
        newEmail: target,
      })
      if (sendError) {
        setError(authErrorMessage(sendError, `Couldn't send the code.`))
        return
      }
      setNewEmail(target)
      setCode(``)
      setStep(`code`)
    } catch {
      setError(`Couldn't send the code.`)
    } finally {
      setBusy(false)
    }
  }

  const confirm = async () => {
    if (code.trim().length < 6) return
    setBusy(true)
    setError(``)
    try {
      const { error: changeError } = await authClient.emailOtp.changeEmail({
        newEmail,
        otp: code.trim(),
      })
      if (changeError) {
        setError(authErrorMessage(changeError, `Couldn't change the email.`))
        return
      }
      // The session cookie cache (5 min) and the module cache (30 s) both
      // still say the old address — bypass both so the chrome updates now.
      invalidateSessionCache()
      await authClient.getSession({ query: { disableCookieCache: true } })
      await refetch()
      onChanged(newEmail)
      close()
    } catch {
      setError(`Couldn't change the email.`)
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) close()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Change your email</DialogTitle>
          <DialogDescription>
            {step === `address`
              ? `We'll send a code to the new address. Sign-in codes, notifications and @mentions use it once confirmed; your old address stops working for sign-in.`
              : `Enter the 6-digit code we sent to ${newEmail}. It expires in 10 minutes.`}
          </DialogDescription>
        </DialogHeader>
        {step === `address` ? (
          <form
            onSubmit={(e) => {
              e.preventDefault()
              void sendCode(newEmail)
            }}
          >
            <DialogBody className="space-y-2">
              <Label htmlFor="new-email">New email</Label>
              <Input
                id="new-email"
                type="email"
                autoComplete="email"
                autoFocus
                required
                value={newEmail}
                onChange={(e) => setNewEmail(e.target.value)}
                placeholder="you@example.com"
              />
              {error && <p className="text-sm text-destructive">{error}</p>}
            </DialogBody>
            <DialogFooter>
              <DialogCancel variant="outline" onClick={close} />
              <Button type="submit" disabled={busy || !newEmail.trim()}>
                {busy ? `Sending…` : `Send code`}
              </Button>
            </DialogFooter>
          </form>
        ) : (
          <form
            onSubmit={(e) => {
              e.preventDefault()
              void confirm()
            }}
          >
            <DialogBody className="space-y-3">
              <div className="space-y-2">
                <Label htmlFor="email-change-code">Code</Label>
                <Input
                  id="email-change-code"
                  type="text"
                  inputMode="numeric"
                  pattern="[0-9]*"
                  autoComplete="one-time-code"
                  autoFocus
                  required
                  maxLength={6}
                  value={code}
                  onChange={(e) => setCode(e.target.value.replace(/\D/g, ``))}
                  placeholder="123456"
                  className="text-center text-lg tracking-[0.4em]"
                />
              </div>
              {error && <p className="text-sm text-destructive">{error}</p>}
              <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                <Button
                  type="button"
                  variant="link"
                  className="h-auto p-0 text-xs text-muted-foreground underline-offset-4 hover:underline"
                  disabled={busy}
                  onClick={() => void sendCode(newEmail)}
                >
                  Resend code
                </Button>
                <Button
                  type="button"
                  variant="link"
                  className="h-auto p-0 text-xs text-muted-foreground underline-offset-4 hover:underline"
                  disabled={busy}
                  onClick={() => {
                    setError(``)
                    setCode(``)
                    setStep(`address`)
                  }}
                >
                  Use a different email
                </Button>
              </div>
            </DialogBody>
            <DialogFooter>
              <DialogCancel variant="outline" onClick={close} />
              <Button type="submit" disabled={busy || code.trim().length < 6}>
                {busy ? `Checking…` : `Confirm`}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
