/* Every variant, size and state of a component as a small live render (the
   enum and boolean props of the docs, over the studio's starting props). */
import { useMemo } from "react"
import { variantsOf, type ComponentDoc, type Specimen } from "../lib/catalog"
import { useScheme } from "../lib/scheme"
import { enumOptions } from "./a2ui"
import type { Backgrounds } from "./backgrounds"
import { caseAlign, studioTree, subjectOf, thumbProps } from "./ComponentStudio"
import { MiniSurface } from "./MiniSurface"

/** The gallery's variants over the studio's small-render props; the page
 *  gates its Variants section on the same list. */
export function galleryVariants(doc: ComponentDoc, specimen: Specimen | undefined) {
  const subject = subjectOf(doc, specimen)
  const base = thumbProps(doc, subject)
  const align = caseAlign(specimen)
  return variantsOf(doc, base, enumOptions).map((v) => ({ ...v, base, tree: studioTree(doc, subject, v.props, align) }))
}

export const hasVariants = (doc: ComponentDoc, specimen: Specimen | undefined) => galleryVariants(doc, specimen).length > 0

export function VariantGallery({ doc, specimen, backgrounds }: { doc: ComponentDoc; specimen: Specimen | undefined; backgrounds: Backgrounds }) {
  const mode = useScheme()
  const variants = useMemo(() => galleryVariants(doc, specimen), [doc, specimen])
  return (
    <div className="sdk-variants">
      {variants.map((v, i) => (
        <figure key={v.label} className="sdk-variant">
          <MiniSurface domId={`variant-${i}`} tree={v.tree} mode={mode} ground={backgrounds.exponential} zoom={0.8} inert />
          <figcaption>
            <code>{v.label}</code>
          </figcaption>
        </figure>
      ))}
    </div>
  )
}
