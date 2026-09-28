// RFC 8628 device codes (EXP-403): the helpers the /auth/device page and the
// Add device dialog's "enter the code the CLI shows" field (EXP-1111) share.

// Better Auth resolves to `{ data, error }` — it does not throw. Map the
// plugin's RFC 8628 error codes to human copy.
export function deviceErrorMessage(
  error: { error?: string; message?: string } | null
): string {
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

// Codes are always 8 chars (printed XXXX-XXXX); the server strips dashes
// and the generated charset is uppercase-only. Auto-insert the dash while
// typing — but only once a 5th char exists, so backspacing over it deletes
// instead of fighting the formatter — and pasting a dashed code stays
// stable because the dash is stripped before re-inserting.
export function normalizeUserCode(input: string): string {
  const bare = input
    .toUpperCase()
    .replace(/[^A-Z0-9]/g, ``)
    .slice(0, 8)
  return bare.length >= 5 ? `${bare.slice(0, 4)}-${bare.slice(4)}` : bare
}

/** A complete code: 8 characters once the dash is stripped. */
export function isCompleteUserCode(code: string): boolean {
  return code.replace(/-/g, ``).length === 8
}
