//! EXP-1321/1324 — THE Queue: the ONE block at the bottom of a session's
//! transcript. Two Trace collapsibles, "n to do" (open: the running work,
//! then the pending tasks) and "n completed" (collapsed). It replaces the
//! bottom strip's background-task + "Waiting on" lines and the Work card's
//! `tasks` variant. Mirror of `packages/domain-contract/src/task-queue.ts`;
//! the tests replay `fixtures/task-queue.json` comparing WHOLE objects.

use serde::{Deserialize, Serialize};

use crate::work_card::work_duration_long;

pub mod copy {
    pub const TODO: &str = "{count} to do";
    pub const COMPLETED: &str = "{count} completed";
    pub const WAITING: &str = "Waiting on {subject}";
}

pub mod icons {
    pub const TODO: &str = "trace-todo";
    pub const COMPLETED: &str = "trace-completed";
    pub const RUNNING: &str = "trace-task-running";
    pub const PENDING: &str = "trace-task-pending";
    pub const DONE: &str = "trace-task-done";
    pub const BACKGROUND: &str = "trace-background";
}

/// `testIds` — the desktop's element ids for the drivers.
pub mod test_ids {
    pub const QUEUE: &str = "session-task-queue";
    pub const TODO: &str = "session-task-queue-todo";
    pub const COMPLETED: &str = "session-task-queue-completed";
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskQueueTask {
    pub content: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueueBackground {
    pub id: String,
    pub kind: String,
    pub description: String,
    #[serde(default)]
    pub started_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskQueueWait {
    pub id: String,
    pub subject: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueueInput {
    /// The agent's own task list, in order.
    pub tasks: Vec<TaskQueueTask>,
    /// The CLI's background tasks.
    pub background_tasks: Vec<TaskQueueBackground>,
    /// Unsettled `wait` rows' subjects.
    pub waits: Vec<TaskQueueWait>,
    /// Epoch ms, for the background durations.
    pub now: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskQueueItemKind {
    Task,
    Background,
    Wait,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskQueueItemState {
    Running,
    Pending,
    Done,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskQueueItem {
    /// `task:<index>`, `background:<id>`, `wait:<id>`.
    pub key: String,
    pub kind: TaskQueueItemKind,
    pub state: TaskQueueItemState,
    pub icon: String,
    pub label: String,
    /// A background task's elapsed time.
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskQueueSection {
    pub title: String,
    pub icon: String,
    pub items: Vec<TaskQueueItem>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskQueue {
    pub todo: TaskQueueSection,
    pub completed: Option<TaskQueueSection>,
}

fn clean(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn count(n: usize, template: &str) -> String {
    template.replace("{count}", &n.to_string())
}

fn item(
    key: String,
    kind: TaskQueueItemKind,
    state: TaskQueueItemState,
    icon: &str,
    label: String,
    description: Option<String>,
) -> TaskQueueItem {
    TaskQueueItem {
        key,
        kind,
        state,
        icon: icon.to_string(),
        label,
        description,
    }
}

/// `None` = no block (nothing to do; an all-completed list hides). "to do"
/// order: in-progress tasks, background tasks, waits, pending tasks. A
/// background task of kind `agent` never shows (a subagent is its own line).
pub fn task_queue(input: &TaskQueueInput) -> Option<TaskQueue> {
    use TaskQueueItemKind as K;
    use TaskQueueItemState as S;
    let mut running = Vec::new();
    let mut pending = Vec::new();
    let mut done = Vec::new();
    for (index, task) in input.tasks.iter().enumerate() {
        let label = clean(&task.content);
        if label.is_empty() {
            continue;
        }
        let key = format!("task:{index}");
        match task.status.as_str() {
            "completed" => done.push(item(key, K::Task, S::Done, icons::DONE, label, None)),
            "in_progress" => {
                running.push(item(key, K::Task, S::Running, icons::RUNNING, label, None))
            }
            _ => pending.push(item(key, K::Task, S::Pending, icons::PENDING, label, None)),
        }
    }
    let background = input.background_tasks.iter().filter_map(|task| {
        if task.kind == "agent" {
            return None;
        }
        let label = clean(&task.description);
        if label.is_empty() {
            return None;
        }
        let description = task
            .started_at
            .map(|started| work_duration_long((input.now - started).max(0) as u64));
        Some(item(
            format!("background:{}", task.id),
            K::Background,
            S::Running,
            icons::BACKGROUND,
            label,
            description,
        ))
    });
    let waits = input.waits.iter().filter_map(|wait| {
        let subject = clean(&wait.subject);
        if subject.is_empty() {
            return None;
        }
        Some(item(
            format!("wait:{}", wait.id),
            K::Wait,
            S::Running,
            icons::BACKGROUND,
            copy::WAITING.replace("{subject}", &subject),
            None,
        ))
    });
    let todo: Vec<TaskQueueItem> =
        running.into_iter().chain(background).chain(waits).chain(pending).collect();
    if todo.is_empty() {
        return None;
    }
    Some(TaskQueue {
        todo: TaskQueueSection {
            title: count(todo.len(), copy::TODO),
            icon: icons::TODO.to_string(),
            items: todo,
        },
        completed: (!done.is_empty()).then(|| TaskQueueSection {
            title: count(done.len(), copy::COMPLETED),
            icon: icons::COMPLETED.to_string(),
            items: done,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/task-queue.json");

    #[test]
    fn every_fixture_case_matches_the_whole_queue() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(cases.len() >= 5);
        for case in cases {
            let input: TaskQueueInput = serde_json::from_value(case["input"].clone()).unwrap();
            let expected: Option<TaskQueue> =
                serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(task_queue(&input), expected, "{}", case["name"]);
        }
    }

    #[test]
    fn the_constants_match_the_fixture() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(fixture["copy"]["todo"], copy::TODO);
        assert_eq!(fixture["copy"]["completed"], copy::COMPLETED);
        assert_eq!(fixture["copy"]["waiting"], copy::WAITING);
        for (key, value) in [
            ("todo", icons::TODO),
            ("completed", icons::COMPLETED),
            ("running", icons::RUNNING),
            ("pending", icons::PENDING),
            ("done", icons::DONE),
            ("background", icons::BACKGROUND),
        ] {
            assert_eq!(fixture["icons"][key], value, "icons.{key}");
        }
        assert_eq!(fixture["testIds"]["queue"], test_ids::QUEUE);
        assert_eq!(fixture["testIds"]["todo"], test_ids::TODO);
        assert_eq!(fixture["testIds"]["completed"], test_ids::COMPLETED);
    }
}
