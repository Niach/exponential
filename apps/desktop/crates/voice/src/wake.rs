//! #NEW-56 (voice plan V7): the wake word, without a wake-word model.
//!
//! While the wake word is on, the listening thread keeps the microphone
//! open and Silero scoring it ([`crate::listen::Segmenter`]); in silence that
//! is all it does. The start of every speech segment ([`window_ms`], sized
//! to the phrase) goes ONCE through the resident Parakeet ([`WakeScanner`] says when: as soon as the
//! open segment is that long, or when a shorter one closes), and the
//! transcript is matched against the member's phrase ([`wake_match`]): the
//! phrase must OPEN the segment. A match opens one turn, the same flow as a
//! push-to-talk key-down: the segment so far seeds an
//! [`crate::listen::Utterance`] that strips the phrase from what it heard,
//! and the turn ends by itself after [`TURN_END_MS`] of quiet
//! ([`turn_over`]).
//!
//! Fixture-locked by `packages/domain-contract/fixtures/talk-listen.json`
//! (`wake`): only the voice device listens, so this is the one
//! implementation.

use crate::listen::{Segmenter, Transcriber, Utterance, Vad, FRAME, RATE};

/// How much of a segment's start is transcribed for the match
/// ([`window_ms`]): sized to the phrase, because Parakeet's cost grows with
/// the audio (owner decision 2026-10-27: a fixed 2 s cost 29 % of a CPU core
/// under continuous talk).
pub const WINDOW_BASE_MS: usize = 600;
pub const WINDOW_PER_LETTER_MS: usize = 100;
pub const WINDOW_MIN_MS: usize = 1_200;
pub const WINDOW_MAX_MS: usize = 2_000;
/// The edits a match tolerates, as a share of the phrase's letters (rounded
/// down: `Hey Studio` = 9 letters, 2 edits).
pub const MAX_EDIT_RATIO: f32 = 0.25;
/// Quiet after the command that ends the turn.
pub const TURN_END_MS: usize = 900;
/// Quiet after a phrase with nothing after it that ends the turn (Empty).
pub const NO_COMMAND_MS: usize = 4_000;
/// The longest turn.
pub const MAX_TURN_MS: usize = 60_000;
/// How long after a reply ends the microphone still counts as hearing it
/// (its echo, the player's buffer): no self-wake.
pub const ECHO_TAIL_MS: usize = 300;

const fn frames(ms: usize) -> usize {
    (ms * RATE / 1000).div_ceil(FRAME)
}

/// One word of a transcript: its folded form and where it sits in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub folded: String,
    pub start: usize,
    pub end: usize,
}

/// Lowercase ASCII for the letters Parakeet writes with accents (a phrase
/// typed `Jurgen` still hears `Jürgen`).
fn fold(ch: char, out: &mut String) {
    let folded = match ch {
        'ä' | 'á' | 'à' | 'â' | 'å' => "a",
        'ö' | 'ó' | 'ò' | 'ô' => "o",
        'ü' | 'ú' | 'ù' | 'û' => "u",
        'é' | 'è' | 'ê' | 'ë' => "e",
        'í' | 'ì' | 'î' | 'ï' => "i",
        'ç' => "c",
        'ñ' => "n",
        'ß' => "ss",
        _ => {
            out.push(ch);
            return;
        }
    };
    out.push_str(folded);
}

/// The words of `text`: runs of letters and digits, lowercased and folded;
/// an apostrophe joins (`What's` = `whats`), anything else separates.
pub fn wake_words(text: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut current: Option<Word> = None;
    for (index, ch) in text.char_indices() {
        if ch.is_alphanumeric() {
            let word = current.get_or_insert_with(|| Word { folded: String::new(), start: index, end: index });
            for lower in ch.to_lowercase() {
                fold(lower, &mut word.folded);
            }
            word.end = index + ch.len_utf8();
        } else if matches!(ch, '\'' | '\u{2019}') && current.is_some() {
            continue;
        } else if let Some(word) = current.take() {
            words.push(word);
        }
    }
    words.extend(current);
    words
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1).min(row[j] + 1).min(diagonal + usize::from(ca != cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The edits [`wake_match`] tolerates for `phrase`: floor(letters ×
/// [`MAX_EDIT_RATIO`]).
pub fn wake_allowed(phrase: &str) -> usize {
    let letters: usize = wake_words(phrase).iter().map(|word| word.folded.chars().count()).sum();
    (letters as f32 * MAX_EDIT_RATIO).floor() as usize
}

/// How close `heard`'s opening comes to `phrase`: the phrase's letters
/// (spaces ignored, so `Heystudio` and `Hey stu dio` count) against the
/// transcript's first 1 to n + 2 words joined (n = the phrase's words), the
/// closest (the fewest words on a tie) as (words, edits). None for a phrase
/// without a letter or digit, or a transcript without a word.
pub fn wake_distance(phrase: &str, heard: &str) -> Option<(usize, usize)> {
    let phrase_words = wake_words(phrase);
    let key: Vec<char> = phrase_words.iter().flat_map(|word| word.folded.chars()).collect();
    if key.is_empty() {
        return None;
    }
    let mut best: Option<(usize, usize)> = None;
    let mut candidate: Vec<char> = Vec::new();
    for (count, word) in wake_words(heard).iter().take(phrase_words.len() + 2).enumerate() {
        candidate.extend(word.folded.chars());
        let distance = levenshtein(&candidate, &key);
        if best.is_none_or(|(_, best)| distance < best) {
            best = Some((count + 1, distance));
        }
    }
    best
}

/// Whether `heard` opens with `phrase`; `Some(rest)` = what follows the
/// phrase (leading punctuation dropped, the first letter capitalised), the
/// command. The closest opening ([`wake_distance`]) matches within
/// [`wake_allowed`] edits. A phrase without a letter or digit never matches.
pub fn wake_match(phrase: &str, heard: &str) -> Option<String> {
    let (count, distance) = wake_distance(phrase, heard)?;
    if distance > wake_allowed(phrase) {
        return None;
    }
    let words = wake_words(heard);
    let rest = heard[words[count - 1].end..].trim_start_matches(|ch: char| !ch.is_alphanumeric());
    let mut chars = rest.chars();
    Some(match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    })
}

/// The window for `phrase`: [`WINDOW_BASE_MS`] + [`WINDOW_PER_LETTER_MS`] per
/// letter or digit, clamped to [`WINDOW_MIN_MS`] … [`WINDOW_MAX_MS`].
pub fn window_ms(phrase: &str) -> usize {
    let letters: usize = wake_words(phrase).iter().map(|word| word.folded.chars().count()).sum();
    (WINDOW_BASE_MS + WINDOW_PER_LETTER_MS * letters).clamp(WINDOW_MIN_MS, WINDOW_MAX_MS)
}

/// What the scanner wants transcribed: frames `[start, end)` of the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Check {
    pub start: usize,
    pub end: usize,
}

/// Speech segments into wake checks, one per segment: when the open segment
/// reaches its window, or when a shorter one closes. Also keeps the
/// audio and scores the next check, and a match's seed, need (the open
/// segment, else only the padding a segment could reach back into).
#[derive(Debug, Clone)]
pub struct WakeScanner {
    /// The window in frames.
    window: usize,
    segmenter: Segmenter,
    /// The open segment was checked already.
    checked: bool,
    /// The first frame kept in `audio` / `scores`.
    base: usize,
    audio: Vec<f32>,
    scores: Vec<f32>,
    /// Frames scored while the voice played: a segment starting before
    /// this is the reply's and never checked.
    played: usize,
}

impl WakeScanner {
    /// A scanner checking the first `window_ms` of each segment.
    pub fn new(window_ms: usize) -> Self {
        WakeScanner {
            window: frames(window_ms),
            segmenter: Segmenter::default(),
            checked: false,
            base: 0,
            audio: Vec::new(),
            scores: Vec::new(),
            played: 0,
        }
    }

    /// Captured samples, before their scores.
    pub fn samples(&mut self, samples: &[f32]) {
        self.audio.extend_from_slice(samples);
    }

    /// Score the next frame; a check that is due comes back, its audio
    /// ([`Self::pcm`]) and seed ([`Self::seed`]) kept until the next push.
    pub fn push(&mut self, probability: f32) -> Option<Check> {
        // What the previous frame left behind (its check is handled by now).
        self.trim();
        self.scores.push(probability);
        let closed = self.segmenter.push(probability);
        let check = match closed {
            Some(segment) if !self.checked => {
                Some(Check { start: segment.start, end: segment.end.min(segment.start + self.window) })
            }
            _ => self.segmenter.open_start().filter(|_| !self.checked).and_then(|start| {
                (self.segmenter.frames() - start >= self.window).then_some(Check { start, end: start + self.window })
            }),
        };
        if closed.is_some() || self.segmenter.open_start().is_none() {
            self.checked = false;
        }
        if check.is_some() && self.segmenter.open_start().is_some() {
            self.checked = true;
        }
        check
    }

    /// The samples of frames `[start, end)` (what is still kept of them).
    pub fn pcm(&self, check: Check) -> &[f32] {
        let from = (check.start.saturating_sub(self.base) * FRAME).min(self.audio.len());
        let to = (check.end.saturating_sub(self.base) * FRAME).min(self.audio.len());
        &self.audio[from..to]
    }

    /// A match at `check`: the audio from its start to now and the scores
    /// of its whole frames, to seed the turn's utterance.
    pub fn seed(&self, check: Check) -> (Vec<f32>, Vec<f32>) {
        let from = check.start.saturating_sub(self.base);
        (self.audio[(from * FRAME).min(self.audio.len())..].to_vec(), self.scores[from.min(self.scores.len())..].to_vec())
    }

    /// Frames scored so far.
    pub fn frames(&self) -> usize {
        self.segmenter.frames()
    }

    /// Drop what no check or seed can reach any more.
    fn trim(&mut self) {
        let keep = self
            .segmenter
            .open_start()
            .unwrap_or_else(|| self.segmenter.frames().saturating_sub(frames(crate::listen::PAD_MS)));
        if keep > self.base {
            let drop = keep - self.base;
            self.audio.drain(..(drop * FRAME).min(self.audio.len()));
            self.scores.drain(..drop.min(self.scores.len()));
            self.base = keep;
        }
    }
}

/// One capture through the scan: score it, transcribe the checks it makes
/// due, and match each against `phrase`. A match comes back with the scores
/// of the capture's frames after it (the turn's utterance takes them).
/// `playing` = a reply is being spoken (or just was, [`ECHO_TAIL_MS`]): the
/// capture is the voice's own, and a segment that starts in it is never
/// checked, so the agent can't wake itself.
#[allow(clippy::too_many_arguments)]
pub fn scan(
    scanner: &mut WakeScanner,
    vad: &mut dyn Vad,
    transcriber: &mut dyn Transcriber,
    samples: &[f32],
    phrase: &str,
    language: &str,
    playing: bool,
) -> Result<Option<(Check, Vec<f32>)>, String> {
    scanner.samples(samples);
    let probabilities = vad.push(samples)?;
    for (index, probability) in probabilities.iter().enumerate() {
        let check = scanner.push(*probability);
        if playing {
            scanner.played = scanner.frames();
        }
        let Some(check) = check else { continue };
        if check.start < scanner.played {
            continue;
        }
        let heard = transcriber.transcribe(scanner.pcm(check), language)?;
        if wake_match(phrase, &heard).is_some() {
            return Ok(Some((check, probabilities[index + 1..].to_vec())));
        }
    }
    Ok(None)
}

/// A match's turn: the segment from the check's start seeds it, the
/// capture's later scores follow.
pub fn turn(
    id: &str,
    language: &str,
    phrase: &str,
    scanner: &WakeScanner,
    (check, rest): (Check, Vec<f32>),
    transcriber: &mut dyn Transcriber,
) -> Result<Utterance, (talk::ListenFailure, String)> {
    let (audio, scores) = scanner.seed(check);
    let mut utterance = Utterance::woken(id, language, phrase, &audio, &scores, transcriber)?;
    utterance.feed_scored(&[], &rest, transcriber, &mut |_| {})?;
    Ok(utterance)
}

/// Whether the turn has heard enough ([`turn_over`]).
pub fn turn_ended(utterance: &Utterance) -> bool {
    turn_over(utterance.open(), utterance.quiet_ms(), utterance.has_command(), (utterance.seconds() * 1000.0) as usize)
}

/// Whether a wake turn is over: never while a segment is open (except past
/// [`MAX_TURN_MS`]); after [`TURN_END_MS`] of quiet once a command was
/// heard, after [`NO_COMMAND_MS`] while only the phrase was.
pub fn turn_over(open: bool, quiet_ms: usize, has_command: bool, elapsed_ms: usize) -> bool {
    if elapsed_ms >= MAX_TURN_MS {
        return true;
    }
    if open {
        return false;
    }
    quiet_ms >= if has_command { TURN_END_MS } else { NO_COMMAND_MS }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/talk-listen.json");

    fn wake() -> serde_json::Value {
        serde_json::from_str::<serde_json::Value>(FIXTURE).unwrap()["wake"].clone()
    }

    #[test]
    fn the_constants_are_the_fixture() {
        let c = &wake()["constants"];
        assert_eq!(c["windowBaseMs"], WINDOW_BASE_MS);
        assert_eq!(c["windowPerLetterMs"], WINDOW_PER_LETTER_MS);
        assert_eq!(c["windowMinMs"], WINDOW_MIN_MS);
        assert_eq!(c["windowMaxMs"], WINDOW_MAX_MS);
        assert_eq!(c["maxEditRatio"].as_f64().unwrap() as f32, MAX_EDIT_RATIO);
        assert_eq!(c["turnEndMs"], TURN_END_MS);
        assert_eq!(c["noCommandMs"], NO_COMMAND_MS);
        assert_eq!(c["maxTurnMs"], MAX_TURN_MS);
        assert_eq!(c["echoTailMs"], ECHO_TAIL_MS);
    }

    #[test]
    fn windows_follow_the_fixture() {
        for case in wake()["window"].as_array().unwrap() {
            assert_eq!(window_ms(case["phrase"].as_str().unwrap()), case["windowMs"].as_u64().unwrap() as usize, "{case}");
        }
    }

    #[test]
    fn words_fold_like_the_fixture() {
        for case in wake()["words"].as_array().unwrap() {
            let words: Vec<String> = wake_words(case["in"].as_str().unwrap()).into_iter().map(|word| word.folded).collect();
            let expected: Vec<String> = serde_json::from_value(case["out"].clone()).unwrap();
            assert_eq!(words, expected, "{case}");
        }
    }

    #[test]
    fn matches_follow_the_fixture() {
        for case in wake()["match"].as_array().unwrap() {
            let got = wake_match(case["phrase"].as_str().unwrap(), case["heard"].as_str().unwrap());
            let expected = case["rest"].as_str().map(str::to_string);
            assert_eq!(got, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn checks_follow_the_fixture() {
        for case in wake()["checks"].as_array().unwrap() {
            let mut scanner = WakeScanner::new(case["windowMs"].as_u64().unwrap() as usize);
            let mut checks = Vec::new();
            for run in case["probabilities"].as_array().unwrap() {
                for _ in 0..run[1].as_u64().unwrap() {
                    scanner.samples(&[0.0; FRAME]);
                    if let Some(check) = scanner.push(run[0].as_f64().unwrap() as f32) {
                        // Every check's audio is still kept.
                        assert_eq!(scanner.pcm(check).len(), (check.end - check.start) * FRAME, "{}", case["name"]);
                        checks.push([check.start, check.end]);
                    }
                }
            }
            let expected: Vec<[usize; 2]> = serde_json::from_value(case["checks"].clone()).unwrap();
            assert_eq!(checks, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn turns_end_like_the_fixture() {
        for case in wake()["turn"].as_array().unwrap() {
            let over = turn_over(
                case["open"].as_bool().unwrap(),
                case["quietMs"].as_u64().unwrap() as usize,
                case["command"].as_bool().unwrap(),
                case["elapsedMs"].as_u64().unwrap() as usize,
            );
            assert_eq!(over, case["over"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    /// A VAD on loudness alone and a transcriber that hears one sentence:
    /// the scan, the match, the seeded turn and its end, as the listener
    /// runs them.
    struct Loud(Vec<f32>);

    impl crate::listen::Vad for Loud {
        fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
            self.0.extend_from_slice(samples);
            let whole = self.0.len() / FRAME * FRAME;
            let out = self.0[..whole].chunks(FRAME).map(|frame| if crate::listen::measure(frame).0 > 0.4 { 0.9 } else { 0.05 }).collect();
            self.0.drain(..whole);
            Ok(out)
        }
    }

    struct Says(&'static str, usize);

    impl crate::listen::Transcriber for Says {
        fn transcribe(&mut self, _pcm: &[f32], _language: &str) -> Result<String, String> {
            self.1 += 1;
            Ok(self.0.to_string())
        }
        fn partials(&self) -> bool {
            false
        }
    }

    fn tone(seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize)
            .map(|index| amplitude * (std::f32::consts::TAU * 440.0 * index as f32 / RATE as f32).sin())
            .collect()
    }

    /// Runs `audio` through the scan and, on a match, the turn: what the
    /// turn answered (None = no wake) and the transcriptions spent.
    fn wake_on(audio: &[f32], says: &'static str) -> (Option<talk::Event>, usize) {
        wake_on_playing(audio, says, 0)
    }

    /// [`wake_on`] while a reply plays over the first `played` samples.
    fn wake_on_playing(audio: &[f32], says: &'static str, played: usize) -> (Option<talk::Event>, usize) {
        let mut vad = Loud(Vec::new());
        let mut engines = Says(says, 0);
        let mut scanner = WakeScanner::new(window_ms("Hey Studio"));
        let mut turn: Option<Utterance> = None;
        let mut answer = None;
        for (index, chunk) in audio.chunks(320).enumerate() {
            let playing = index * 320 < played;
            if let Some(utterance) = turn.as_mut() {
                utterance.feed(chunk, &mut vad, &mut engines, &mut |_| {}).unwrap();
                if turn_ended(utterance) {
                    turn.take().unwrap().finish(&mut engines, &mut |event| answer = Some(event)).unwrap();
                    break;
                }
                continue;
            }
            if let Some(matched) = scan(&mut scanner, &mut vad, &mut engines, chunk, "Hey Studio", "en", playing).unwrap() {
                turn = Some(super::turn("wake-1", "en", "Hey Studio", &scanner, matched, &mut engines).unwrap());
            }
        }
        (answer, engines.1)
    }

    #[test]
    fn a_phrase_and_a_command_are_one_turn_without_the_phrase() {
        let audio = [tone(0.5, 0.0), tone(1.2, 0.3), tone(1.5, 0.0)].concat();
        let (answer, transcriptions) = wake_on(&audio, "Hey Studio, move NEW-78 to done.");
        assert_eq!(answer, Some(talk::Event::Heard { id: "wake-1".into(), text: "Move NEW-78 to done.".into() }));
        assert_eq!(transcriptions, 2, "the check, then the turn's segment");
    }

    #[test]
    fn a_phrase_alone_ends_empty_after_the_wait() {
        let audio = [tone(0.3, 0.0), tone(0.8, 0.3), tone(5.0, 0.0)].concat();
        let (answer, _) = wake_on(&audio, "Hey Studio.");
        assert_eq!(answer, Some(talk::Event::Empty { id: "wake-1".into() }));
    }

    #[test]
    fn other_speech_is_transcribed_once_per_segment_and_never_wakes() {
        let audio = [tone(0.3, 0.0), tone(0.8, 0.3), tone(0.6, 0.0), tone(3.0, 0.3), tone(0.6, 0.0), tone(10.0, 0.0)].concat();
        let (answer, transcriptions) = wake_on(&audio, "They studied it.");
        assert_eq!(answer, None);
        assert_eq!(transcriptions, 2, "one check per segment; silence costs none");
    }

    /// #1263 review: the agent's own reply (heard back through the
    /// microphone) never wakes it, also when its segment runs past the end
    /// of the playback; the person's phrase after it does.
    #[test]
    fn the_voice_never_wakes_itself() {
        let said = "Hey Studio, move NEW-78 to done.";
        let reply = [tone(0.3, 0.0), tone(1.2, 0.3), tone(1.5, 0.0)].concat();
        assert_eq!(wake_on_playing(&reply, said, reply.len()), (None, 0), "inside the playback: never transcribed");
        let overrun = [tone(0.3, 0.0), tone(2.5, 0.3), tone(1.5, 0.0)].concat();
        let ends = (0.3 + 1.0) * RATE as f32;
        assert_eq!(wake_on_playing(&overrun, said, ends as usize).0, None, "began in the playback");
        let after = [tone(0.3, 0.3), tone(1.0, 0.0), tone(1.2, 0.3), tone(1.5, 0.0)].concat();
        let ends = 0.4 * RATE as f32;
        let (answer, _) = wake_on_playing(&after, said, ends as usize);
        assert_eq!(answer, Some(talk::Event::Heard { id: "wake-1".into(), text: "Move NEW-78 to done.".into() }), "the person's phrase after it");
    }

    #[test]
    fn the_seed_starts_at_the_check_and_runs_to_now() {
        let mut scanner = WakeScanner::new(2_000);
        let mut found = None;
        for (index, probability) in [0.0f32; 20].into_iter().chain([0.9; 70]).enumerate() {
            scanner.samples(&[index as f32; FRAME]);
            found = found.or(scanner.push(probability));
        }
        let check = found.expect("a 2 s segment is checked while open");
        assert_eq!(check, Check { start: 18, end: 18 + frames(2_000) });
        let (audio, scores) = scanner.seed(check);
        assert_eq!(scores.len(), 90 - 18);
        assert_eq!(audio.len(), scores.len() * FRAME);
        assert_eq!(audio[0], 18.0, "the padded start");
    }
}
