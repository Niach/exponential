// EXP-1169: the device-setup block's server card is the SAME component on all
// four clients: the install command in a box with a copy control, and, once
// it was copied, the field that approves a CLI device code in place. These
// are its words; `device-setup-copy.test.ts` reads the native sources off
// disk to prove they say the same thing:
//
//   desktop  crates/ui/src/device_setup.rs
//   iOS      UI/Components/DeviceSetupCopy.swift
//   Android  ui/components/DeviceSetupCopy.kt
//
// The card's title and description are the getting-started `server` entry
// (already gated). Keep every string free of double quotes and backslashes:
// the drift test matches them as literals inside Swift/Kotlin/Rust source.
export const DEVICE_SETUP_COPY = {
  copyCommand: `Copy install command`,
  codeLabel: `If the CLI shows a code, enter it here`,
  codePlaceholder: `XXXX-XXXX`,
  approve: `Approve`,
  approved: `Code approved. The CLI signs in within a few seconds.`,
  codeUsed: `That code has already been used. Run the login command again.`,
  codeExpired: `That code has expired. Run the login command again to get a new one.`,
  codeInvalid: `That code isn't valid. Check for typos, or run the login command again.`,
  codeOtherAccount: `This code was requested from a different account.`,
  failed: `Something went wrong. Try again.`,
} as const
