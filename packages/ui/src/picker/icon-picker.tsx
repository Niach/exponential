// EXP-1029 contract — `IconPicker(set)` under the shared picker API.
//
// The existing `../icon-picker.tsx` (EXP-575: a square trigger opening the
// swatch grid, EXP-924: the SET is a parameter — `options`, the board set
// unless a surface names `DEVICE_ICON_OPTIONS`) already IS the typed picker
// of this contract; EXP-1021 re-homes it here on top of `Picker` (the grid
// as the surface's body, the trigger unchanged). Until then this file
// forwards it so the barrel resolves ONE symbol and the contract test names
// it.
export { IconPicker } from "../icon-picker"
