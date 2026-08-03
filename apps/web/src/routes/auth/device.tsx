import * as React from "react"
import { createFileRoute, redirect } from "@tanstack/react-router"
import { useState } from "react"
import { authClient, fetchSessionOnce } from "@/lib/auth/client"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { AuthFormShell } from "@/components/auth-form-shell"

// RFC 8628 device verification page (EXP-403): the `exponential` CLI prints
// `Visit <instance>/auth/device and enter XXXX-XXXX`. Claiming happens via
// GET /api/auth/device?user_code=… (which binds the code to THIS signed-in
// session — approve/deny reject unclaimed codes), then the user explicitly
// approves or denies. Login-guarded: an anonymous visit bounces through
// /auth/login with a redirect back here, user_code intact.
export const Route = createFileRoute(`/auth/device`)({
  component: DeviceVerificationPage,
  ssr: false,
  validateSearch: (search: Record<string, unknown>): { user_code?: string } => ({
    // The code charset is 2-9 digits + letters, so an all-digit code like
    // `23456789` is legal — and the router JSON-parses search values, which
    // turns it into a NUMBER. Accept both or the prefill silently drops.
    user_code:
      typeof search.user_code === `string`
        ? search.user_code
        : typeof search.user_code === `number`
          ? String(search.user_code)
          : undefined,
  }),
  beforeLoad: async ({ location }) => {
    const session = await fetchSessionOnce()
    if (!session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
})

type Step = `enter` | `confirm` | `approved` | `denied`

// Better Auth resolves to `{ data, error }` — it does not throw. Map the
// plugin's RFC 8628 error codes to human copy.
function deviceErrorMessage(error: { error?: string; message?: string } | null): string {
  const code = error?.error ?? ``
  if (code === `expired_token`) {
    return `That code has expired. Run the login command again to get a new one.`
  }
  if (code === `invalid_request` || code === `invalid_grant`) {
    return `That code isn't valid. Check for typos, or run the login command again.`
  }
  if (code === `access_denied`) {
    return `This code was requested from a different account.`
  }
  return error?.message || `Something went wrong. Try again.`
}

// The user types the code as printed (XXXX-XXXX); the server strips dashes
// and the generated charset is uppercase-only.
export function normalizeUserCode(input: string): string {
  return input.toUpperCase().replace(/[^A-Z0-9-]/g, ``)
}

export function DeviceVerificationView({
  initialCode,
}: {
  initialCode?: string
}) {
  const [step, setStep] = useState<Step>(`enter`)
  const [code, setCode] = useState(() => normalizeUserCode(initialCode ?? ``))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(``)

  const claim = async (e?: React.FormEvent) => {
    e?.preventDefault()
    if (!code || busy) return
    setBusy(true)
    setError(``)
    // GET /api/auth/device claims the pending code for this session — the
    // approve/deny endpoints refuse codes no session has claimed.
    const result = await authClient.device({
      query: { user_code: code },
    })
    setBusy(false)
    if (result.error) {
      setError(deviceErrorMessage(result.error))
      return
    }
    const status = result.data?.status
    if (status === `approved` || status === `denied`) {
      setError(`That code has already been used. Run the login command again.`)
      return
    }
    setStep(`confirm`)
  }

  const decide = async (approve: boolean) => {
    if (busy) return
    setBusy(true)
    setError(``)
    const result = approve
      ? await authClient.device.approve({ userCode: code })
      : await authClient.device.deny({ userCode: code })
    setBusy(false)
    if (result.error) {
      setError(deviceErrorMessage(result.error))
      return
    }
    setStep(approve ? `approved` : `denied`)
  }

  if (step === `approved`) {
    return (
      <div className="space-y-2 text-center">
        <p className="text-sm">Device connected.</p>
        <p className="text-sm text-muted-foreground">
          You can close this tab and return to your terminal.
        </p>
      </div>
    )
  }

  if (step === `denied`) {
    return (
      <div className="space-y-2 text-center">
        <p className="text-sm">Request denied.</p>
        <p className="text-sm text-muted-foreground">
          The device was not signed in. You can close this tab.
        </p>
      </div>
    )
  }

  if (step === `confirm`) {
    return (
      <div className="space-y-4">
        <div className="rounded-md border bg-muted/30 p-3 text-center font-mono text-lg tracking-widest">
          {code}
        </div>
        <p className="text-sm text-muted-foreground">
          A device showing this code is asking to sign in to your account. It
          will get the same access as you have here. Only approve if the code
          matches what your terminal shows.
        </p>
        {error && <p className="text-sm text-destructive">{error}</p>}
        <div className="flex gap-2">
          <Button
            variant="outline"
            className="flex-1"
            disabled={busy}
            onClick={() => void decide(false)}
          >
            Deny
          </Button>
          <Button
            className="flex-1"
            disabled={busy}
            onClick={() => void decide(true)}
          >
            Approve
          </Button>
        </div>
      </div>
    )
  }

  return (
    <form onSubmit={(e) => void claim(e)} className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="device-user-code">Code</Label>
        <Input
          id="device-user-code"
          value={code}
          onChange={(e) => setCode(normalizeUserCode(e.target.value))}
          placeholder="XXXX-XXXX"
          autoFocus
          autoComplete="off"
          spellCheck={false}
          className="text-center font-mono text-lg tracking-widest"
        />
      </div>
      {error && <p className="text-sm text-destructive">{error}</p>}
      <Button type="submit" className="w-full" disabled={busy || !code}>
        Continue
      </Button>
    </form>
  )
}

function DeviceVerificationPage() {
  const { user_code: userCode } = Route.useSearch()
  return (
    <AuthFormShell
      title="Connect a device"
      description="Enter the code shown in your terminal"
      footer={null}
    >
      <DeviceVerificationView initialCode={userCode} />
    </AuthFormShell>
  )
}
