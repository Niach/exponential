//! The ring's pure rules, each replaying its fixture cases: DEDUPE
//! (`dedupeCases`), RETENTION (`retentionCases`), PAUSE (`pauseCases`),
//! SELECT (`selectCases`) and the tool's FIT (`fitCases`).

use std::collections::HashMap;

use crate::fixture;
use crate::hash::distance;

/// The last KEPT frame of each display: what a new frame is compared to.
#[derive(Default)]
pub(crate) struct Dedupe {
    last: HashMap<u32, (String, String, u64)>,
}

impl Dedupe {
    /// Whether the frame is kept (and becomes the display's last kept one).
    #[cfg(test)]
    pub(crate) fn keep(&mut self, display: u32, app: &str, title: &str, hash: u64) -> bool {
        let kept = self.kept(display, app, title, hash);
        if kept {
            self.remember(display, app, title, hash);
        }
        kept
    }

    /// Whether the frame would be kept, without remembering it.
    pub(crate) fn kept(&self, display: u32, app: &str, title: &str, hash: u64) -> bool {
        !self.last.get(&display).is_some_and(|(last_app, last_title, last_hash)| {
            last_app == app && last_title == title && distance(*last_hash, hash) <= fixture::max_distance()
        })
    }

    /// The frame was stored: it is the display's last kept one now.
    pub(crate) fn remember(&mut self, display: u32, app: &str, title: &str, hash: u64) {
        self.last.insert(display, (app.to_string(), title.to_string(), hash));
    }
}

/// The oldest `at` (ms) still kept at `now`: a frame with at < this goes.
pub(crate) fn cutoff(now: i64) -> i64 {
    now - fixture::retention_ms()
}

/// A frame taken at `at` (ms) is gone at `now`.
#[cfg(test)]
pub(crate) fn expired(at: i64, now: i64) -> bool {
    at < cutoff(now)
}

/// Whether the focused window holds capture paused (`denylist`): the app
/// id or name contains a denylisted app, or the title a denylisted title;
/// case-insensitive.
pub(crate) fn denied(app: &str, app_name: &str, title: &str) -> bool {
    let (app, app_name, title) = (app.to_lowercase(), app_name.to_lowercase(), title.to_lowercase());
    fixture::denylist("apps").iter().any(|entry| app.contains(entry.as_str()) || app_name.contains(entry.as_str()))
        || fixture::denylist("titles").iter().any(|entry| title.contains(entry.as_str()))
}

/// What SELECT reads of a frame.
pub(crate) struct Candidate<'a> {
    pub at: i64,
    pub app: &'a str,
    pub title: &'a str,
}

/// The indexes of the frames the tool returns, by time: from <= at <= to,
/// matching `filter` (a case-insensitive substring of the app or title),
/// spread to at most `tool.maxFrames`.
pub(crate) fn select(frames: &[Candidate<'_>], from: i64, to: i64, filter: Option<&str>) -> Vec<usize> {
    let filter = filter.map(str::to_lowercase).filter(|filter| !filter.is_empty());
    let mut matching: Vec<usize> = (0..frames.len())
        .filter(|&index| {
            let frame = &frames[index];
            from <= frame.at
                && frame.at <= to
                && filter.as_ref().is_none_or(|filter| {
                    frame.app.to_lowercase().contains(filter.as_str()) || frame.title.to_lowercase().contains(filter.as_str())
                })
        })
        .collect();
    matching.sort_by_key(|&index| (frames[index].at, index));
    spread(&matching, fixture::max_frames())
}

/// `n` items over `max`: indexes round(i * (n - 1) / (max - 1)), half away
/// from zero.
fn spread(items: &[usize], max: usize) -> Vec<usize> {
    let n = items.len();
    if n <= max {
        return items.to_vec();
    }
    if max <= 1 {
        return items.iter().take(max).copied().collect();
    }
    (0..max).map(|i| items[(i as f64 * (n - 1) as f64 / (max - 1) as f64).round() as usize]).collect()
}

/// The size a frame of `width` x `height` (cropped to `crop` = (width,
/// height) of the crop when it has one) is stored and goes out at: long
/// edge at most `tool.maxEdge`, rounded to nearest, never scaled up.
pub(crate) fn fit(width: u32, height: u32, crop: Option<(u32, u32)>) -> (u32, u32) {
    let (width, height) = crop.unwrap_or((width, height));
    let long = width.max(height);
    let max = fixture::max_edge();
    if long <= max || long == 0 {
        return (width, height);
    }
    let scale = |side: u32| ((u64::from(side) * u64::from(max) * 2 + u64::from(long)) / (2 * u64::from(long))).max(1) as u32;
    (scale(width), scale(height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::fixture;
    use crate::hash::parse;

    #[test]
    fn dedupe_replays_every_dedupe_case() {
        for case in fixture()["dedupeCases"].as_array().unwrap() {
            let mut dedupe = Dedupe::default();
            let kept: Vec<bool> = case["frames"]
                .as_array()
                .unwrap()
                .iter()
                .map(|frame| {
                    dedupe.keep(
                        frame["display"].as_u64().unwrap() as u32,
                        frame["app"].as_str().unwrap(),
                        frame["title"].as_str().unwrap(),
                        parse(frame["hash"].as_str().unwrap()).unwrap(),
                    )
                })
                .collect();
            let expect: Vec<bool> = case["kept"].as_array().unwrap().iter().map(|kept| kept.as_bool().unwrap()).collect();
            assert_eq!(kept, expect, "{}", case["name"]);
        }
    }

    #[test]
    fn retention_replays_every_retention_case() {
        for case in fixture()["retentionCases"].as_array().unwrap() {
            let now = case["now"].as_i64().unwrap();
            let kept: Vec<bool> = case["frames"].as_array().unwrap().iter().map(|at| !expired(at.as_i64().unwrap(), now)).collect();
            let expect: Vec<bool> = case["kept"].as_array().unwrap().iter().map(|kept| kept.as_bool().unwrap()).collect();
            assert_eq!(kept, expect, "{}", case["name"]);
        }
    }

    #[test]
    fn pause_replays_every_pause_case() {
        for case in fixture()["pauseCases"].as_array().unwrap() {
            let got = denied(case["app"].as_str().unwrap(), case["appName"].as_str().unwrap(), case["title"].as_str().unwrap());
            assert_eq!(got, case["paused"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    pub(crate) fn ms(rfc3339: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(rfc3339).unwrap().timestamp_millis()
    }

    #[test]
    fn select_replays_every_select_case() {
        for case in fixture()["selectCases"].as_array().unwrap() {
            let frames: Vec<Candidate<'_>> = case["frames"]
                .as_array()
                .unwrap()
                .iter()
                .map(|frame| Candidate {
                    at: ms(frame["at"].as_str().unwrap()),
                    app: frame["app"].as_str().unwrap(),
                    title: frame["title"].as_str().unwrap(),
                })
                .collect();
            let picked = select(
                &frames,
                ms(case["from"].as_str().unwrap()),
                ms(case["to"].as_str().unwrap()),
                case["filter"].as_str(),
            );
            let expect: Vec<usize> = case["picked"].as_array().unwrap().iter().map(|i| i.as_u64().unwrap() as usize).collect();
            assert_eq!(picked, expect, "{}", case["name"]);
        }
    }

    #[test]
    fn fit_replays_every_fit_case() {
        for case in fixture()["fitCases"].as_array().unwrap() {
            let crop = case["crop"].as_object().map(|crop| (crop["width"].as_u64().unwrap() as u32, crop["height"].as_u64().unwrap() as u32));
            let got = fit(case["width"].as_u64().unwrap() as u32, case["height"].as_u64().unwrap() as u32, crop);
            let expect = (case["expect"]["width"].as_u64().unwrap() as u32, case["expect"]["height"].as_u64().unwrap() as u32);
            assert_eq!(got, expect, "{}", case["name"]);
        }
    }

    #[test]
    fn a_spread_of_five_over_four_rounds_half_away_from_zero() {
        // i * 4 / 3 = 0, 1.33, 2.67, 4.
        assert_eq!(spread(&[10, 11, 12, 13, 14], 4), [10, 11, 13, 14]);
        // n = 7: i * 6 / 3 = 0, 2, 4, 6; n = 3 over 4 = all.
        assert_eq!(spread(&[0, 1, 2, 3, 4, 5, 6], 4), [0, 2, 4, 6]);
        assert_eq!(spread(&[0, 1, 2], 4), [0, 1, 2]);
    }
}
