// Maps Better Auth error codes to clear, user-facing messages. Unknown codes
// fall back to the server-provided message so real failures stay diagnosable.
const AUTH_ERROR_MESSAGES: Record<string, string> = {
  INVALID_EMAIL_OR_PASSWORD: `Incorrect email or password.`,
  INVALID_PASSWORD: `Incorrect email or password.`,
  USER_NOT_FOUND: `Incorrect email or password.`,
  USER_ALREADY_EXISTS: `An account with this email already exists. Sign in instead.`,
  PASSWORD_TOO_SHORT: `Password is too short. Use at least 8 characters.`,
  PASSWORD_TOO_LONG: `Password is too long.`,
  INVALID_EMAIL: `Enter a valid email address.`,
  // EXP-857 one-time codes (email-otp plugin).
  INVALID_OTP: `That code is not right. Check the email and try again.`,
  OTP_EXPIRED: `That code expired. Request a new one.`,
  TOO_MANY_ATTEMPTS: `Too many attempts. Request a new code.`,
  // EXP-857 passkeys (@better-auth/passkey).
  PASSKEY_NOT_FOUND: `That passkey is not registered here. Continue another way, then add it under Account.`,
  AUTHENTICATION_FAILED: `Passkey check failed. Try again.`,
  CHALLENGE_NOT_FOUND: `That passkey attempt expired. Try again.`,
  AUTH_CANCELLED: `Passkey sign-in was cancelled.`,
}

export function authErrorMessage(
  error: { code?: string; message?: string } | null | undefined,
  fallback: string
): string {
  if (!error) return fallback
  if (error.code && AUTH_ERROR_MESSAGES[error.code]) {
    return AUTH_ERROR_MESSAGES[error.code]
  }
  return error.message || fallback
}
