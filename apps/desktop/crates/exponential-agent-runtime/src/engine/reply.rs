//! EXP-1292 / EXP-1426: the cards an agent reply carries — what the mapper
//! produces beside a turn's final text (a question, a finished subagent).
//! Pure data; a host posts them as they serialize (Exponential:
//! `sessions.reply`, the server's `MessageCard` JSON).

/// A card on an agent reply — serializes to the server's `MessageCard` JSON
/// (`kind` tag, camelCase keys). `questionId` rides the tolerant passthrough
/// card schema so an answered card maps back to the engine's pending
/// question without device-side state.
///
/// Serialized by hand (not derived): a question card's options go out as
/// `options: string[]` (labels) plus the EXP-1322 `optionDescriptions`
/// sidecar, see [`option_descriptions`].
#[derive(Clone, Debug, PartialEq)]
pub enum ReplyCard {
    Question {
        question: String,
        options: Vec<ReplyOption>,
        question_id: String,
    },
    Subagent {
        title: String,
        status: String,
        summary: Option<String>,
    },
}

impl serde::Serialize for ReplyCard {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Self::Question {
                question,
                options,
                question_id,
            } => {
                // EXP-1322: options post as label strings; descriptions ride
                // the `optionDescriptions` sidecar, never inside `options`.
                let labels: Vec<&str> = options.iter().map(ReplyOption::label).collect();
                let descriptions = option_descriptions(options);
                let mut map =
                    serializer.serialize_map(Some(4 + usize::from(descriptions.is_some())))?;
                map.serialize_entry("kind", "question")?;
                map.serialize_entry("question", question)?;
                map.serialize_entry("options", &labels)?;
                if let Some(descriptions) = &descriptions {
                    map.serialize_entry("optionDescriptions", descriptions)?;
                }
                map.serialize_entry("questionId", question_id)?;
                map.end()
            }
            Self::Subagent {
                title,
                status,
                summary,
            } => {
                let mut map = serializer.serialize_map(Some(3 + usize::from(summary.is_some())))?;
                map.serialize_entry("kind", "subagent")?;
                map.serialize_entry("title", title)?;
                map.serialize_entry("status", status)?;
                if let Some(summary) = summary {
                    map.serialize_entry("summary", summary)?;
                }
                map.end()
            }
        }
    }
}

/// EXP-1322: one option of a question card: its label and, when the feed
/// option carries one, its description (the thread card shows it like the
/// session view's card). The answer names the option by its label.
///
/// On the wire the card posts `options` as plain label strings and the
/// descriptions in the parallel `optionDescriptions` sidecar
/// ([`option_descriptions`]); the enum itself never goes on the wire.
#[derive(Clone, Debug, PartialEq)]
pub enum ReplyOption {
    Label(String),
    Described { label: String, description: String },
}

impl ReplyOption {
    /// A feed option: described only when the description carries text.
    pub fn new(label: impl Into<String>, description: Option<&str>) -> Self {
        let label = label.into();
        match description.map(str::trim).filter(|text| !text.is_empty()) {
            Some(description) => Self::Described { label, description: description.to_string() },
            None => Self::Label(label),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Label(label) | Self::Described { label, .. } => label,
        }
    }

    /// The description, `None` for a plain label.
    pub fn description(&self) -> Option<&str> {
        match self {
            Self::Label(_) => None,
            Self::Described { description, .. } => Some(description),
        }
    }
}

/// The EXP-1322 `optionDescriptions` sidecar of a question card: parallel to
/// `options` (same length), `None` = that option has no description; `None`
/// altogether (the key is omitted) when no option carries one.
pub fn option_descriptions(options: &[ReplyOption]) -> Option<Vec<Option<&str>>> {
    let descriptions: Vec<Option<&str>> = options.iter().map(ReplyOption::description).collect();
    descriptions.iter().any(Option::is_some).then_some(descriptions)
}

impl From<&str> for ReplyOption {
    fn from(label: &str) -> Self {
        Self::Label(label.to_string())
    }
}

impl From<String> for ReplyOption {
    fn from(label: String) -> Self {
        Self::Label(label)
    }
}
