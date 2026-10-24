//! EXP-1029 contract — the assignee picker: the team's members by avatar +
//! name, the email as the description and a search keyword; `Unassigned`
//! first. `pickers::assignee_*` (EXP-288) moves onto this.
//!
//! #EXP-1383 (agent-settings.json `assignee`): after the members, the team's
//! AGENTS (`agents`, already in `assignee_options` order: sortOrder, then
//! name), straight after the members with no heading; each row = THE AGENT
//! MARK + the name, which search matches. An agent row's value is
//! `agent:<id>` ([`agent_value`]); a member's stays the bare user id, so
//! [`assignee_from_value`] splits a pick into the two exclusive columns.

use gpui::AnyElement;

use domain::agent_settings::{AgentActorRow, ASSIGNEE_AGENT_PREFIX};
use domain::rows::User;

use super::{OnPickerChange, Picker, PickerItem, PickerMode};

/// The row that clears the pick.
pub(crate) const UNASSIGNED_VALUE: &str = "";

/// An agent row's picker value.
pub(crate) fn agent_value(agent_id: &str) -> String {
    format!("{ASSIGNEE_AGENT_PREFIX}{agent_id}")
}

/// The single picker's value for an issue's two exclusive columns
/// ([`UNASSIGNED_VALUE`] when neither is set).
pub(crate) fn assignee_value(assignee_id: Option<&str>, assignee_agent_id: Option<&str>) -> String {
    match api::issues::Assignee::of(assignee_id, assignee_agent_id) {
        api::issues::Assignee::Agent(id) => agent_value(&id),
        api::issues::Assignee::User(id) => id,
        api::issues::Assignee::Nobody => UNASSIGNED_VALUE.to_string(),
    }
}

/// A pick back into the columns: an agent clears the person, a person (or
/// `Unassigned`) clears the agent.
pub(crate) fn assignee_from_value(value: Option<&str>) -> api::issues::Assignee {
    match value {
        Some(value) if value.starts_with(ASSIGNEE_AGENT_PREFIX) => {
            api::issues::Assignee::Agent(value[ASSIGNEE_AGENT_PREFIX.len()..].to_string())
        }
        Some(value) if value != UNASSIGNED_VALUE => api::issues::Assignee::User(value.to_string()),
        _ => api::issues::Assignee::Nobody,
    }
}

/// The solo-team rule (`assignee.$comment`): a team with exactly one member
/// hides its assignee control only while it has no agents (an agent is
/// someone to assign to) and the issue carries no agent assignee.
pub(crate) fn hides_assignee(member_count: usize, agent_count: usize, has_agent_assignee: bool) -> bool {
    member_count == 1 && agent_count == 0 && !has_agent_assignee
}

pub(crate) fn assignee_items(
    members: &[User],
    agents: &[AgentActorRow],
    allows_none: bool,
) -> Vec<PickerItem<String>> {
    let rows = members.iter().map(|member| {
        let email = member.email.clone().unwrap_or_default();
        let name = member.name.clone().unwrap_or_else(|| email.clone());
        PickerItem::new(member.id.clone(), name.clone())
            .description(email.clone())
            .keywords(vec![name.into(), email.into()])
    });
    let agent_rows = agents.iter().map(|agent| {
        PickerItem::new(agent_value(&agent.id), agent.name.clone()).keywords(vec![agent.name.clone().into()])
    });
    let none = allows_none.then(|| PickerItem::new(UNASSIGNED_VALUE.to_string(), "Unassigned"));
    none.into_iter().chain(rows).chain(agent_rows).collect()
}

/// `mode`: single for the issue chip / dialog (an empty `value` = unassigned;
/// `allows_none` offers the `Unassigned` row, which reports an EMPTY set),
/// multi for the board filter (`allows_none` ignored).
pub(crate) fn assignee_picker(
    members: &[User],
    agents: &[AgentActorRow],
    mode: PickerMode,
    value: Vec<String>,
    allows_none: bool,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    // The avatar is what makes a member row a MEMBER row, and it is not a
    // `PickerItem` slot — a picker glyph is an icon, never a photo. So the
    // one row body that needs one draws it over the primitive's, and
    // `Unassigned` keeps the plain body: there is nobody to picture.
    let avatars: Vec<(String, String, Option<String>)> = members
        .iter()
        .map(|member| {
            let email = member.email.clone().unwrap_or_default();
            let name = member.name.clone().unwrap_or_else(|| email.clone());
            (member.id.clone(), name, member.image.clone())
        })
        .collect();
    // #EXP-1383: an agent row wears THE AGENT MARK at the member avatar's
    // size.
    let marks: Vec<(String, Option<String>, Option<String>)> = agents
        .iter()
        .map(|agent| (agent_value(&agent.id), agent.icon.clone(), agent.color.clone()))
        .collect();
    let picker = match mode {
        PickerMode::Single => Picker::single(
            assignee_items(members, agents, allows_none),
            value.into_iter().next(),
            trigger,
            on_change,
        ),
        PickerMode::Multi => {
            Picker::multi(assignee_items(members, agents, false), value, trigger, on_change)
        }
    };
    picker
        .search(true)
        .empty_text("No members")
        .render_item(move |item, cx| {
            use gpui::{IntoElement as _, ParentElement as _, Styled as _};
            let mark = if let Some((_, icon, color)) = marks.iter().find(|(value, _, _)| *value == item.value) {
                crate::user_avatar::agent_avatar(
                    icon.as_deref(),
                    color.as_deref(),
                    gpui_component::Size::XSmall,
                    cx,
                )
            } else if let Some((id, name, image)) = avatars.iter().find(|(id, _, _)| *id == item.value) {
                crate::user_avatar::user_avatar(
                    id,
                    name,
                    image.as_deref(),
                    gpui_component::Size::XSmall,
                    cx,
                )
            } else {
                return super::picker_item_body(item, cx);
            };
            let body = super::picker_item_body(item, cx);
            gpui_component::h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_2()
                .child(mark)
                .child(body)
                .into_any_element()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, name: &str) -> AgentActorRow {
        AgentActorRow { id: id.into(), name: name.into(), icon: None, color: None }
    }

    /// `assignee.optionCases` order: unassigned, the members, then the agents
    /// (the caller hands them in `assignee_options` order).
    #[test]
    fn agents_follow_the_members() {
        let members: Vec<User> = vec![serde_json::from_value(serde_json::json!({"id": "u-1", "name": "Danny"})).unwrap()];
        let items = assignee_items(&members, &[agent("g-1", "Sloppy"), agent("g-2", "Zed")], true);
        let values: Vec<&str> = items.iter().map(|item| item.value.as_str()).collect();
        assert_eq!(values, vec![UNASSIGNED_VALUE, "u-1", "agent:g-1", "agent:g-2"]);
    }

    #[test]
    fn a_pick_splits_into_the_two_columns() {
        assert_eq!(assignee_from_value(Some("agent:g-1")), api::issues::Assignee::Agent("g-1".into()));
        assert_eq!(assignee_from_value(Some("u-1")), api::issues::Assignee::User("u-1".into()));
        assert_eq!(assignee_from_value(Some(UNASSIGNED_VALUE)), api::issues::Assignee::Nobody);
        assert_eq!(assignee_from_value(None), api::issues::Assignee::Nobody);
        assert_eq!(assignee_value(None, Some("g-1")), "agent:g-1");
        assert_eq!(assignee_value(Some("u-1"), None), "u-1");
        assert_eq!(assignee_value(None, None), UNASSIGNED_VALUE);
    }

    /// A solo team keeps the assignee control while it has agents.
    #[test]
    fn a_solo_team_with_agents_keeps_the_control() {
        assert!(hides_assignee(1, 0, false));
        assert!(!hides_assignee(1, 1, false));
        assert!(!hides_assignee(1, 0, true));
        assert!(!hides_assignee(2, 0, false));
        assert!(!hides_assignee(0, 0, false));
    }
}
