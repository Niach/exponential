import * as React from "react"
import { createFileRoute, redirect } from "@tanstack/react-router"
import { useState } from "react"
import { authClient, fetchSessionOnce } from "@/lib/auth/client"
import { trpc } from "@/lib/trpc-client"
import { Button, FEEDBACK_WORDS, FieldError, Input, Label, AuthFormShell } from "@exp/ui"
import { failedAction } from "@/lib/action-error"
import { pageTitle } from "@/lib/page-title"
import { deviceErrorMessage, normalizeUserCode } from "@/lib/auth/device-code"
import { DEVICE_SETUP_COPY } from "@/lib/device-setup-copy"

// RFC 8628 device verification page (EXP-403): the `exponential` CLI prints
// `Visit <instance>/auth/device and enter XXXX-XXXX`. Claiming happens via
// GET /api/auth/device?user_code=… (which binds the code to THIS signed-in
// session — approve/deny reject unclaimed codes), then the user explicitly
// approves or denies. Before Approve the page names the asking client and
// what it gets, both read server-side off the claimed `device_codes` row
// (`devices.pendingLogin`, GHSA-q84f-53jg-9ppm), never from the URL.
// Login-guarded: an anonymous visit bounces through
// /auth/login with a redirect back here, user_code intact.
export const Route = createFileRoute(`/auth/device`)({
  head: () => ({ meta: [{ title: pageTitle(`Connect a device`) }] }),
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

// The shared code helpers (the device-setup block uses them too, EXP-1111);
// re-exported so the page's tests keep importing from here.
export { normalizeUserCode }

export function DeviceVerificationView({
  initialCode,
  onSettled,
}: {
  initialCode?: string
  /** Fires when the flow reaches a terminal step (approved or denied) so
   *  the page can drop the "Connect a device" header above the outcome. */
  onSettled?: () => void
}) {
  const [step, setStep] = useState<Step>(`enter`)
  const [code, setCode] = useState(() => normalizeUserCode(initialCode ?? ``))
  const [busy, setBusy] = useState(false)
  // The allow-listed client's name, from the server lookup.
  const [clientName, setClientName] = useState(``)
  const [deciding, setDeciding] = useState<`approve` | `deny` | null>(null)
  // The typed code was refused: inline under the field. A failed
  // approve/deny (no field left) is a toast.
  const [error, setError] = useState(``)

  const claim = async (e?: React.FormEvent) => {
    e?.preventDefault()
    if (!code || busy) return
    setBusy(true)
    setError(``)
    try {
      await claimCode()
    } finally {
      setBusy(false)
    }
  }

  const claimCode = async () => {
    // GET /api/auth/device claims the pending code for this session — the
    // approve/deny endpoints refuse codes no session has claimed.
    const result = await authClient.device({
      query: { user_code: code },
    })
    if (result.error) {
      setError(deviceErrorMessage(result.error))
      return
    }
    const status = result.data?.status
    if (status === `approved` || status === `denied`) {
      setError(DEVICE_SETUP_COPY.codeUsed)
      return
    }
    // Who is asking: the claimed row's client, checked server-side. A
    // client outside the allow-list cannot be approved here.
    // The claim just answered `pending`, so NOT_FOUND here means another
    // account's session claimed the code first.
    const request = await trpc.devices.pendingLogin
      .query({ userCode: code })
      .catch((err: unknown) =>
        (err as { data?: { code?: string } })?.data?.code === `NOT_FOUND`
          ? (`other` as const)
          : null
      )
    if (!request || request === `other`) {
      setError(request ? DEVICE_SETUP_COPY.codeOtherAccount : DEVICE_SETUP_COPY.failed)
      return
    }
    if (!request.clientName) {
      setError(DEVICE_SETUP_COPY.codeInvalid)
      return
    }
    setClientName(request.clientName)
    setStep(`confirm`)
  }

  const decide = async (approve: boolean) => {
    if (busy) return
    setBusy(true)
    setDeciding(approve ? `approve` : `deny`)
    const result = approve
      ? await authClient.device.approve({ userCode: code })
      : await authClient.device.deny({ userCode: code })
    setBusy(false)
    setDeciding(null)
    if (result.error) {
      failedAction(approve ? `approve` : `deny`, `the device`, undefined, {
        description: deviceErrorMessage(result.error),
      })
      return
    }
    setStep(approve ? `approved` : `denied`)
    onSettled?.()
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
        <p className="text-sm">
          <span className="font-medium">{clientName}</span> is asking to sign
          in to your account.
        </p>
        <div className="rounded-md border bg-muted/30 p-3 text-center font-mono text-lg tracking-widest">
          {code}
        </div>
        <p className="text-sm text-muted-foreground">
          Approving gives it a full session as you, with the same access you
          have here. Only approve if the code matches what your terminal shows.
        </p>
        <div className="flex gap-2">
          <Button
            variant="outline"
            className="flex-1"
            pending={deciding === `deny`}
            disabled={busy}
            onClick={() => void decide(false)}
          >
            Deny
          </Button>
          <Button
            className="flex-1"
            pending={deciding === `approve`}
            pendingLabel={FEEDBACK_WORDS.pending.connect}
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
          onChange={(e) => {
            setCode(normalizeUserCode(e.target.value))
            if (error) setError(``)
          }}
          placeholder="XXXX-XXXX"
          maxLength={9}
          autoFocus
          autoComplete="off"
          spellCheck={false}
          className="text-center font-mono text-lg tracking-widest"
          aria-invalid={error ? true : undefined}
        />
        <FieldError>{error}</FieldError>
      </div>
      <Button type="submit" className="w-full" pending={busy} disabled={!code}>
        Continue
      </Button>
    </form>
  )
}

function DeviceVerificationPage() {
  const { user_code: userCode } = Route.useSearch()
  // Once the flow settles the outcome copy stands alone — keeping "Enter
  // the code shown in your terminal" above "Device connected." reads like
  // a further instruction.
  const [settled, setSettled] = useState(false)
  return (
    <AuthFormShell
      title={settled ? undefined : `Connect a device`}
      description={settled ? undefined : `Enter the code shown in your terminal`}
      footer={null}
    >
      <DeviceVerificationView
        initialCode={userCode}
        onSettled={() => setSettled(true)}
      />
    </AuthFormShell>
  )
}
