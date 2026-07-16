// Stand-in for @remotion/google-fonts/Inter, swapped in via vite alias +
// tsconfig paths: the marketing site self-hosts Inter (fonts.css), so the
// video surfaces must not trigger Google Fonts requests at runtime.
type LoadFontOptions = {
  weights?: string[]
  subsets?: string[]
  ignoreTooManyRequestsWarning?: boolean
}

export const loadFont = (
  _style?: string,
  _options?: LoadFontOptions,
): { fontFamily: string } => ({ fontFamily: `Inter` })
