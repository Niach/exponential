//! #EXP-1778 (`fixtures/text-truncate.json`): a Text `truncate: "middle"`
//! line (start…end), the rule gpui's `TruncateMiddle` applies (eui-gpui
//! paints through `text_ellipsis_middle`; the TS twin is
//! packages/exponential-ui/src/text-truncate.ts). Per-character widths are
//! summed; a line that fits shows whole, else the `…` takes its width off the
//! box and the rest splits 2:1 between the longest start and the longest end
//! that fit; no room for either = the `…` alone.

/// The shown line of `text` in `width` px, cut in the middle.
pub fn middle_truncate_width(text: &str, width: f64, measure: impl Fn(char) -> f64, affix: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let widths: Vec<f64> = chars.iter().map(|c| measure(*c)).collect();
    if widths.iter().sum::<f64>() <= width {
        return text.to_string();
    }
    let budget = width - affix.chars().map(&measure).sum::<f64>();
    if budget <= 0.0 {
        return affix.to_string();
    }
    let front_budget = budget * (2.0 / 3.0);
    let back_budget = budget - front_budget;
    let (mut front, mut front_w) = (0usize, 0.0);
    while front < chars.len() && front_w + widths[front] <= front_budget {
        front_w += widths[front];
        front += 1;
    }
    let (mut back, mut back_w) = (chars.len(), 0.0);
    while back > 0 && back_w + widths[back - 1] <= back_budget {
        back -= 1;
        back_w += widths[back];
    }
    if front >= back {
        return affix.to_string();
    }
    format!("{}{affix}{}", chars[..front].iter().collect::<String>(), chars[back..].iter().collect::<String>())
}
