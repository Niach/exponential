// EXP-1029 contract — the account picker under the shared picker API.
//
// The EXP-991 picker (`../account-picker.tsx`: brand mark + login email per
// row, the EXP-992 rate-limit preview bars under the picked login) MOVES here
// on top of `Picker` in EXP-1021, keeping its props and its preview. Until
// then this file forwards the existing component so the barrel's
// `export * from "./picker"` and `export * from "./account-picker"` resolve
// to ONE symbol, and the typed-picker contract test names it.
//
// Target shape (EXP-1021): `AccountPicker` renders
// `<Picker mode="single" items={accountPickerItems(options)} …/>` with the
// limit bars as each row's `description`, and `variant` picking the trigger
// (`inline` = the composer's muted word, `row` = the glass picker row).
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
