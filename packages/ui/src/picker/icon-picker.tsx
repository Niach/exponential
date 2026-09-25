// EXP-1029 contract — `IconPicker(set)` under the shared picker API.
//
// `../icon-picker.tsx` (EXP-575: a square trigger opening the swatch grid,
// EXP-924: the SET is a parameter — `options`, the board set unless a
// surface names `DEVICE_ICON_OPTIONS`) IS the typed picker of this contract;
// it is re-exported here so the picker barrel names every typed picker and
// the contract test can name it.
export { IconPicker } from "../icon-picker"
