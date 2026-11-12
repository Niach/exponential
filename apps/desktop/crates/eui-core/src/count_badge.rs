//! #FEED-232: THE count badge's label (the `CountBadge` native, the
//! Segmented rail's `count`, the gpui `count_badge`; mirrors
//! `src/count-badge.ts`, domain-contract nav-badges.json `labelCases`).
//! Zero or less (or not a number) = `""` = NOTHING renders; past `max` it
//! reads `{max}+`.

use serde_json::Value;

/// The default cap.
pub const COUNT_BADGE_MAX: u64 = 99;

/// What the badge prints for `count` under `max` (`""` = no badge).
pub fn label(count: Option<&Value>, max: Option<&Value>) -> String {
    let n = count.and_then(Value::as_f64).filter(|n| n.is_finite()).map(f64::floor).unwrap_or(0.0);
    if n <= 0.0 {
        return String::new();
    }
    let cap = max.and_then(Value::as_f64).filter(|m| m.is_finite() && *m >= 1.0).map(f64::floor).unwrap_or(COUNT_BADGE_MAX as f64);
    if n > cap {
        format!("{cap}+")
    } else {
        format!("{n}")
    }
}

/// [`label`] of a plain count at the default cap.
pub fn label_of(count: u64) -> String {
    label(Some(&Value::from(count)), None)
}

/// The tone a prop names: `attention`, else `muted`.
pub fn is_attention(tone: Option<&Value>) -> bool {
    tone.and_then(Value::as_str) == Some("attention")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nav_badges_label_cases() {
        for (count, expected) in [(0, ""), (-3, ""), (1, "1"), (99, "99"), (100, "99+"), (1234, "99+")] {
            assert_eq!(label(Some(&json!(count)), None), expected, "{count}");
        }
    }

    #[test]
    fn max_fractions_and_junk() {
        assert_eq!(label(Some(&json!(120)), Some(&json!(9))), "9+");
        assert_eq!(label(Some(&json!(9)), Some(&json!(9))), "9");
        assert_eq!(label(Some(&json!(2.7)), None), "2");
        assert_eq!(label(Some(&json!(0.5)), None), "");
        assert_eq!(label(Some(&json!("3")), None), "");
        assert_eq!(label(None, None), "");
        assert_eq!(label(Some(&json!(120)), Some(&json!(0))), "99+");
        assert_eq!(label_of(7), "7");
        assert!(is_attention(Some(&json!("attention"))));
        assert!(!is_attention(Some(&json!("primary"))));
    }
}
