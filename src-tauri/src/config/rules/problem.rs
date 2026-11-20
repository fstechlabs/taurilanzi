use serde::Serialize;

/// Why a configuration is refused, in words precise enough for whoever sent it to fix it: the
/// field, the value it had and what it takes. For callers of the API; the UI shows its own texts.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Problem(String);

impl Problem {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// The same problem, found inside `place`: `actions[2]: …`.
    pub(super) fn within(self, place: impl std::fmt::Display) -> Self {
        Self(format!("{place}: {}", self.0))
    }
}

pub(super) fn fail<T>(message: impl Into<String>) -> Result<T, Problem> {
    Err(Problem::new(message))
}

/// The name a tagged value has in JSON, such as `open_url` for an action.
pub(super) fn tag(value: &impl Serialize, field: &str) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|json| json.get(field)?.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// A text field the UI caps at `max` characters.
pub(super) fn at_most(field: &str, text: &str, max: usize) -> Result<(), Problem> {
    let length = text.chars().count();
    if length > max {
        return fail(format!(
            "{field} can be at most {max} characters, got {length}"
        ));
    }
    Ok(())
}

pub(super) fn within_range<T: PartialOrd + Copy + std::fmt::Display>(
    field: &str,
    value: T,
    (min, max): (T, T),
) -> Result<(), Problem> {
    if !(min..=max).contains(&value) {
        return fail(format!("{field} must be {min} to {max}, got {value}"));
    }
    Ok(())
}

pub(super) fn finite(field: &str, value: f32) -> Result<(), Problem> {
    if !value.is_finite() {
        return fail(format!("{field} must be a number, got {value}"));
    }
    Ok(())
}
