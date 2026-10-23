/* The one live A2UI surface of "UI as data, down to the compositor"
   (EXP-1369): the expense form the desktop draft shows being built, written
   as a tree of the core catalog and painted in the page by the Exponential
   UI React renderer (components/blog/LiveSurface). scripts/blog-stills.ts
   photographs the live render into public/blog/<id>.webp for the prerender
   and feed readers. The three screen drafts of the post are not surfaces:
   they are drawn in HTML and CSS (components/blog/concepts). */
import type { Nested } from "./a2ui"

/** The expense form: nine components, the whole window of the desktop draft. */
export const EXPENSE_SURFACE: Nested = {
  id: `root`,
  component: `Card`,
  props: { title: `Log an expense`, description: `a form to log an expense, with a place to drop the receipt` },
  children: [
    {
      id: `form`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `sm`, align: `stretch` },
      children: [
        { id: `amount`, component: `NumberField`, props: { label: `Amount`, value: 42, precision: 2, unit: `€` } },
        {
          id: `category`,
          component: `Select`,
          props: {
            label: `Category`,
            value: `travel`,
            options: [
              { value: `travel`, label: `Travel` },
              { value: `meals`, label: `Meals` },
              { value: `office`, label: `Office` },
            ],
          },
        },
        { id: `date`, component: `DatePicker`, props: { label: `Date`, value: `2026-10-24` } },
        { id: `receipt`, component: `FileUpload`, props: { label: `Receipt`, hint: `Drop the receipt here`, accept: `image/*,application/pdf` } },
        {
          id: `actions`,
          component: `Stack`,
          props: { direction: `horizontal`, gap: `xs`, align: `center`, justify: `end` },
          children: [
            { id: `cancel`, component: `Button`, props: { label: `Cancel`, variant: `ghost`, size: `sm` } },
            { id: `save`, component: `Button`, props: { label: `Save`, variant: `default`, size: `sm` } },
          ],
        },
      ],
    },
  ],
}
