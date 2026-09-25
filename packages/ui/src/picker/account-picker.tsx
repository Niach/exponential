// EXP-1029 contract — the account picker under the shared picker API.
//
// The EXP-991 picker (`../account-picker.tsx`: brand mark + login email per
// row, the EXP-992 rate-limit preview bars under the picked login) is
// re-exported here so the picker barrel names every typed picker: the
// barrel's `export * from "./picker"` and `export * from "./account-picker"`
// resolve to ONE symbol, and the typed-picker contract test names it.
export {
  AccountPicker,
  AccountLimitBars,
  AccountOptionLabel,
  ACCOUNT_LIMIT_LABELS,
  accountLimitBars,
  limitTone,
  type AccountLimits,
  type AccountPickerOption,
} from "../account-picker"
