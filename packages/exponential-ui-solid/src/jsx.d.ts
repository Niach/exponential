// FEED-128: the `attr:` attributes the painters write to match React's DOM
// (React reflects a controlled input's `checked` / `value` as attributes,
// Solid sets only the property; see natives/shared.tsx rule 2).
import "solid-js"

declare module "solid-js" {
  namespace JSX {
    interface ExplicitAttributes {
      checked: string | undefined
      value: string | undefined
    }
  }
}
