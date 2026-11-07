/* The plans on /pricing/, all genUI: a Segmented billing switch over one
   Card per plan. Hydrated on view so the switch flips the Team price
   (yearly €12, monthly €15); the build ships the yearly prices. */
import { useEffect, useMemo } from "react"
import { ExponentialSurface, useSurface } from "@exponential_app/eui-react"
import type { HostPlugin } from "@exponential_app/eui-react"
import type { NestedNode } from "@exponential_app/eui"
import { PLANS } from "./plans.ts"

const BILLING = [
  { label: `Yearly`, value: `yearly` },
  { label: `Monthly`, value: `monthly` },
]
const TEAM = { yearly: { amount: `€12`, cadence: `per seat a month, billed yearly` }, monthly: { amount: `€15`, cadence: `per seat a month, billed monthly` } }

const card = (plan: (typeof PLANS)[number]): NestedNode => ({
  id: `plan_${plan.id}`,
  component: `Card`,
  children: [
    {
      id: `plan_${plan.id}_body`,
      component: `Stack`,
      props: { gap: `lg` },
      children: [
        {
          id: `plan_${plan.id}_head`,
          component: `Stack`,
          props: { gap: `xs` },
          children: [
            { id: `plan_${plan.id}_name`, component: `Text`, props: { text: plan.name, variant: `label` } },
            { id: `plan_${plan.id}_amount`, component: `Heading`, props: { text: plan.id === `team` ? { "@path": `/team/amount` } : plan.amount, level: `h2` } },
            { id: `plan_${plan.id}_cadence`, component: `Text`, props: { text: plan.id === `team` ? { "@path": `/team/cadence` } : plan.cadence, variant: `muted` } },
          ],
        },
        {
          id: `plan_${plan.id}_features`,
          component: `Stack`,
          props: { gap: `sm` },
          children: [
            ...plan.features.map((f, i) => ({
              id: `plan_${plan.id}_f${i}`,
              component: `Stack`,
              props: { direction: `horizontal`, gap: `sm` },
              children: [
                { id: `plan_${plan.id}_f${i}_icon`, component: `Icon`, props: { name: `ui-check`, size: `sm` } },
                { id: `plan_${plan.id}_f${i}_text`, component: `Text`, props: { text: f } },
              ],
            })),
            ...(plan.caveat
              ? [
                  {
                    id: `plan_${plan.id}_caveat`,
                    component: `Stack`,
                    props: { direction: `horizontal`, gap: `sm` },
                    children: [
                      { id: `plan_${plan.id}_caveat_icon`, component: `Icon`, props: { name: `ui-warning`, size: `sm`, tone: `muted` } },
                      { id: `plan_${plan.id}_caveat_text`, component: `Text`, props: { text: plan.caveat, variant: `muted` } },
                    ],
                  },
                ]
              : []),
          ],
        },
        { id: `plan_${plan.id}_cta`, component: `Button`, props: { label: plan.cta.label, href: plan.cta.href, variant: plan.highlight ? `default` : `outline` } },
      ],
    },
  ],
})

const TREE: NestedNode = {
  id: `plans`,
  component: `Stack`,
  props: { gap: `xl`, align: `center` },
  children: [
    { id: `billing`, component: `Segmented`, props: { items: BILLING, value: { "@path": `/billing` } } },
    { id: `cards`, component: `Grid`, props: { columns: { base: 1, md: 3 }, gap: `md` }, style: { width: `100%` }, children: PLANS.map(card) },
  ],
}

export default function PricingPlans() {
  const surface = useSurface({ surfaceId: `plans`, initial: TREE, data: { billing: `yearly`, team: TEAM.yearly } })
  const { setData } = surface
  const billing = (surface.data as { billing?: string }).billing === `monthly` ? `monthly` : `yearly`
  useEffect(() => setData(`/team`, TEAM[billing]), [billing, setData])
  const host = useMemo<HostPlugin>(() => ({ onInput: () => Promise.resolve(), onAction: () => Promise.resolve() }), [])
  return <ExponentialSurface id="xui-plans" surface={surface} theme="exponential" mode="dark" host={host} />
}
