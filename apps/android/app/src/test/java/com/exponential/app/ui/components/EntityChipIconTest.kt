package com.exponential.app.ui.components

import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.EntityPreview
import com.exponential.app.domain.EntityRef
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-920: every entity-preview concept resolves to a glyph in this build —
 * the `when` in [entityConceptIcon] is hand-written, so the contract's kind
 * list is what keeps it complete.
 */
class EntityChipIconTest {

    @Test
    fun `every contract kind draws a concept glyph`() {
        DomainContract.entityRefKindValues.forEach { kind ->
            val concept = EntityPreview.ICON.getValue(kind)
            assertNotNull("$kind → $concept", entityConceptIcon(concept))
            assertNotNull(kind, entityConceptIcon(EntityPreview.refIcon(EntityRef(kind, "x"))))
        }
        // A list of an unknown member kind, and an unknown kind itself, fall
        // back to the list glyph — never to nothing.
        assertNotNull(entityConceptIcon(EntityPreview.refIcon(EntityRef("list", "thing"))))
        assertNotNull(entityConceptIcon(EntityPreview.refIcon(EntityRef("thing", "x"))))
        assertNull(entityConceptIcon("not-a-concept"))
    }
}
