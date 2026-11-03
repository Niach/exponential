package com.exponential.app.domain

// VAPP-234: the template gallery's list and copy, the Android mirror of
// `packages/domain-contract/fixtures/app-templates.json` (`AppTemplatesTest`
// locks them to it). Picking one is the SERVER's (`apps.importTemplate`:
// the pinned signature, then the import).
object AppTemplates {
    data class Template(val id: String, val name: String, val icon: String, val color: String, val description: String)

    object Copy {
        const val TITLE = "Start from a template"
        const val FROM_TEMPLATE = "Start from a template"
        const val INCLUDE_RECORDS = "Include sample records"
        const val NOTE = "From a template"
    }

    object TestIds {
        const val ROW = "app-template"
        const val RECORDS = "app-template-records"
        const val FROM_TEMPLATE = "app-from-template"
    }

    val TEMPLATES = listOf(
        Template("time-tracker", "Time tracker", "clock", "#10b981", "Hours per day and week against a monthly target"),
        Template("crm", "CRM", "briefcase", "#6366f1", "Companies, contacts and the deal pipeline"),
        Template("standup", "Standup", "users", "#f59e0b", "The async daily standup"),
    )
}
