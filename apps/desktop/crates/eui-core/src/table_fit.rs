//! #FEED-224 (`fixtures/table-fit.json`): THE column fit of a Markdown
//! table, the twin of packages/exponential-ui/src/table-fit.ts. Each column
//! comes measured in content px (`min` = its widest unbreakable piece, a chip
//! or code span whole; `max` = its widest cell on one line); the fit returns
//! whole-pixel content widths: their sum is under `available` when every
//! column fits on one line (the table hugs its content) and over it when the
//! minimums do not fit (the table scrolls sideways).

/// The widest a column's min-content counts (the fixture's `wordCap`).
pub const TABLE_WORD_CAP: f64 = 160.0;

/// One column's measured content widths.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FitColumn {
    pub min: f64,
    pub max: f64,
}

/// The content width of each column `available` px of content wide.
pub fn fit_table_columns(columns: &[FitColumn], available: f64, cap: f64) -> Vec<f64> {
    let avail = available.max(0.0).floor();
    let lo: Vec<f64> = columns.iter().map(|c| c.min.max(0.0).ceil().min(cap)).collect();
    let hi: Vec<f64> = columns.iter().zip(&lo).map(|(c, l)| c.max.max(0.0).ceil().max(*l)).collect();
    let sum = |xs: &[f64]| xs.iter().sum::<f64>();
    if sum(&lo) >= avail {
        return lo;
    }
    if sum(&hi) <= avail {
        return hi;
    }
    // Water-fill: one level L with Σ clamp(L, lo, hi) = avail.
    let at = |level: f64| lo.iter().zip(&hi).map(|(l, h)| level.max(*l).min(*h)).sum::<f64>();
    let mut points: Vec<f64> = lo.iter().chain(&hi).copied().collect();
    points.sort_by(f64::total_cmp);
    points.dedup();
    let mut k = 0;
    while at(points[k + 1]) < avail {
        k += 1;
    }
    let a = points[k];
    let growing: Vec<usize> = (0..lo.len()).filter(|&i| lo[i] <= a && hi[i] > a).collect();
    let level = a + (avail - at(a)) / growing.len() as f64;
    let mut out: Vec<f64> = lo.iter().zip(&hi).map(|(l, h)| level.floor().max(*l).min(*h)).collect();
    let mut left = avail - sum(&out);
    for i in growing {
        if left <= 0.0 {
            break;
        }
        if out[i] < hi[i] {
            out[i] += 1.0;
            left -= 1.0;
        }
    }
    out
}
