// Minimal `react-native` stand-in so react-strict-dom's NATIVE build loads in
// a headless Bun process. Only what its import chain touches at module scope
// or inside css.create/css.props; components are inert placeholders.
const Stub = (name: string) => {
  const C = (props: unknown) => ({ type: name, props })
  ;(C as any).displayName = name
  return C
}
const noopSub = { remove() {} }

export const Platform = {
  OS: `ios`,
  Version: `18.0`,
  constants: { reactNativeVersion: { major: 0, minor: 81, patch: 0 } },
  select: <T,>(o: { ios?: T; default?: T }) => o.ios ?? o.default,
}
export const StyleSheet = {
  create: <T,>(s: T) => s,
  flatten: (s: unknown): Record<string, unknown> =>
    Array.isArray(s) ? Object.assign({}, ...s.map(StyleSheet.flatten)) : ((s as any) ?? {}),
  hairlineWidth: 1,
}
export const PixelRatio = { get: () => 3, getFontScale: () => 1 }
export const Appearance = { getColorScheme: () => `dark`, addChangeListener: () => noopSub }
export const I18nManager = { isRTL: false }
export const AccessibilityInfo = {
  addEventListener: () => noopSub,
  isReduceMotionEnabled: async () => false,
}
class AnimatedValue {
  constructor(public v: number) {}
  interpolate() {
    return this
  }
}
export const Animated = {
  Value: AnimatedValue,
  View: Stub(`Animated.View`),
  Text: Stub(`Animated.Text`),
  Image: Stub(`Animated.Image`),
  createAnimatedComponent: (c: unknown) => c,
  timing: () => ({ start() {}, stop() {} }),
  spring: () => ({ start() {}, stop() {} }),
}
const ease = (t: number) => t
export const Easing = {
  ease,
  linear: ease,
  bezier: () => ease,
  in: () => ease,
  out: () => ease,
  inOut: () => ease,
}
export const useWindowDimensions = () => ({ width: 390, height: 844, scale: 3, fontScale: 1 })
export const useColorScheme = () => `dark`
export const Text = Stub(`Text`)
export const View = Stub(`View`)
export const TextInput = Stub(`TextInput`)
export const Image = Stub(`Image`)
export const Pressable = Stub(`Pressable`)
export const experimental_LayoutConformance = Stub(`LayoutConformance`)
export default { Platform, StyleSheet }
