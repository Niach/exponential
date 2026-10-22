// FEED-128 (the inputs + select lanes): the extra `attr:` / `prop:` names
// these painters write to match React's DOM: React syncs a controlled
// input's / textarea's `defaultValue` (the `value` attribute, the textarea
// text) with its value, sets a native <option>'s `selected` attribute from
// a <select defaultValue>, and leaves `style=""` on Radix's slider thumb.
import "solid-js"

declare module "solid-js" {
  namespace JSX {
    interface ExplicitAttributes {
      selected: string | undefined
      style: string | undefined
    }
    interface ExplicitProperties {
      defaultValue: string | undefined
    }
  }
}
