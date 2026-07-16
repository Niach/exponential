// Stand-in for @remotion/google-fonts/JetBrainsMono — see inter-shim.ts.
type LoadFontOptions = {
  weights?: string[]
  subsets?: string[]
  ignoreTooManyRequestsWarning?: boolean
}

export const loadFont = (
  _style?: string,
  _options?: LoadFontOptions,
): { fontFamily: string } => ({ fontFamily: `JetBrains Mono` })
