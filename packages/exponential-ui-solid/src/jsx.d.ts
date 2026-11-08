// FEED-128: the `attr:` attributes the painters write to match React's DOM
// (React reflects a controlled input's `checked` / `value` as attributes,
// Solid sets only the property; see natives/shared.tsx rule 2).
import "solid-js"

declare module "solid-js" {
  namespace JSX {
    interface ExplicitAttributes {
      checked: string | undefined
      value: string | undefined
      // FEED-200: a swipeable Tabs' neighbour panels (React's `inert`).
      inert: string | undefined
    }
    // An overlay trigger's toggle runs on the way down (natives/overlays.tsx).
    interface CustomCaptureEvents {
      click: MouseEvent
    }
  }
}
