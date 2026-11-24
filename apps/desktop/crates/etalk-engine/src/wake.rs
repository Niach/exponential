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
//! S-3 SLIDING WINDOW: a segment that stays open (continuous talk, a TV)
//! is checked again every [`WINDOW_HOP_MS`] over its LAST window, and once
//! more when it closes with at least half a hop unchecked, at most
//! [`MAX_CHECKS_PER_SEGMENT`] times; such a later check opens mid-speech, so
//! it matches with [`slide_match`] (the phrase within the first
//! [`SLIDE_SKIP_WORDS`] + 1 words, [`SLIDE_EDIT_RATIO`]).
//!
//! EXP-1424 MODEL DETECTOR: a phrase with a published model
//! (`etalk_wakeword::models`, e.g. spezl's `Hey spezl`) whose files are
//! ready is heard by its model ([`SetDetector`]) instead: every 80 ms frame scored by
//! the openWakeWord pipeline, no Parakeet pass while scanning. Both sit
//! behind [`WakeDetector`]; every other phrase keeps [`TranscriptMatcher`]
//! (all of the above).
//!
//! Fixture-locked by `packages/etalk/fixtures/talk-listen.json`
//! (`wake`, `wake.detector`): only the voice device listens, so this is the
//! one implementation.

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
/// #NEW-86: how long after a wake turn ends the microphone still counts as
/// the turn's (a trailing phrase, the command's echo): no second turn at once.
pub const COOLDOWN_MS: usize = 1_500;
/// S-3: an open segment is checked again each time it grew this far past
/// the previous check's end.
pub const WINDOW_HOP_MS: usize = 1_000;
/// S-3: the checks of one segment, the first included.
pub const MAX_CHECKS_PER_SEGMENT: usize = 6;
/// S-3: a later check's tolerance (stricter than [`MAX_EDIT_RATIO`]: it
/// transcribes more windows and lets the phrase start later).
pub const SLIDE_EDIT_RATIO: f32 = 0.15;
/// S-3: a later check's phrase may start after this many words (fragments
/// of the speech the window cut into).
pub const SLIDE_SKIP_WORDS: usize = 3;

/// EXP-1424: a model's detection seeds its turn from at most this far back
/// (the phrase lies inside the classifier's window, which ends there).
pub const MODEL_HISTORY_MS: usize = 2_000;
/// EXP-1424: the edits [`model_match`] tolerates when it strips the phrase
/// (the model already heard it: only the transcript's spelling is in doubt).
pub const MODEL_EDIT_RATIO: f32 = 0.5;
/// #NEW-126: a model scores a frame only while the room is not quiet: Silero
/// gave some frame at least this in the last [`MODEL_GATE_HOLD_MS`]. Far
/// under [`crate::listen::THRESHOLD`]: a phrase Silero scores lower than
/// this throughout could open no segment, so its turn would hear nothing.
pub const MODEL_GATE: f32 = 0.1;
/// #NEW-126: how long after the last such frame the model keeps scoring: its
/// classifier's window (1.28 s), so every frame that could still hold the
/// phrase is scored.
pub const MODEL_GATE_HOLD_MS: usize = 1_280;

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
    wake_match_edits(phrase, heard).map(|(_, rest)| rest)
}

/// [`wake_match`] with the edits its opening took.
fn wake_match_edits(phrase: &str, heard: &str) -> Option<(usize, String)> {
    let (count, distance) = wake_distance(phrase, heard)?;
    (distance <= wake_allowed(phrase)).then(|| (distance, rest_after(heard, count)))
}

/// The text after `heard`'s first `words` words, leading punctuation
/// dropped, its first letter capitalised.
fn rest_after(heard: &str, words: usize) -> String {
    let all = wake_words(heard);
    let rest = heard[all[words - 1].end..].trim_start_matches(|ch: char| !ch.is_alphanumeric());
    let mut chars = rest.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// S-3: the edits [`slide_match`] tolerates: floor(letters ×
/// [`SLIDE_EDIT_RATIO`]) (`Hey Studio` = 1; under 7 letters, none).
pub fn slide_allowed(phrase: &str) -> usize {
    let letters: usize = wake_words(phrase).iter().map(|word| word.folded.chars().count()).sum();
    (letters as f32 * SLIDE_EDIT_RATIO).floor() as usize
}

/// S-3: a LATER check's match (talk-listen.json `slideMatch`): the window
/// opens mid-speech, so the phrase may start at any of `heard`'s first
/// [`SLIDE_SKIP_WORDS`] + 1 words ([`wake_distance`] from there, the closest
/// wins, the earliest on a tie) and matches within [`slide_allowed`] edits.
/// `Some(rest)` = what follows it, as [`wake_match`].
pub fn slide_match(phrase: &str, heard: &str) -> Option<String> {
    slide_match_edits(phrase, heard).map(|(_, rest)| rest)
}

/// [`slide_match`] with the edits its opening took.
fn slide_match_edits(phrase: &str, heard: &str) -> Option<(usize, String)> {
    let words = wake_words(heard);
    let mut best: Option<(usize, usize)> = None;
    for skip in 0..=SLIDE_SKIP_WORDS.min(words.len()) {
        let Some(word) = words.get(skip) else { break };
        let Some((count, distance)) = wake_distance(phrase, &heard[word.start..]) else { continue };
        if best.is_none_or(|(_, best)| distance < best) {
            best = Some((skip + count, distance));
        }
    }
    let (count, distance) = best?;
    (distance <= slide_allowed(phrase)).then(|| (distance, rest_after(heard, count)))
}

/// EXP-1859 (talk-listen.json `wake.multi`): the most agents the wake word
/// listens for at once.
pub const MAX_LISTENERS: usize = 5;

/// EXP-1859: one agent the wake word listens for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    /// Empty = the one phrase of a Wake without listeners.
    pub agent_id: String,
    pub phrase: String,
    pub language: &'static str,
}

impl Listener {
    pub fn new(agent_id: impl Into<String>, phrase: impl Into<String>, language: &str) -> Self {
        Listener { agent_id: agent_id.into(), phrase: phrase.into(), language: wake_language(language) }
    }
}

/// The language a phrase is heard in: `de`, else `en`.
pub fn wake_language(language: &str) -> &'static str {
    if language == "de" { "de" } else { "en" }
}

/// EXP-1859 (`multiWindow`): the scan's window for several phrases: the
/// longest of their [`window_ms`].
pub fn multi_window_ms<'a>(phrases: impl IntoIterator<Item = &'a str>) -> usize {
    phrases.into_iter().map(window_ms).max().unwrap_or(WINDOW_MIN_MS)
}

/// EXP-1859 (`passes`): the languages a check transcribes its window in:
/// the listeners' distinct ones ([`wake_language`]), in listener order.
pub fn passes<'a>(languages: impl IntoIterator<Item = &'a str>) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for language in languages {
        let language = wake_language(language);
        if !out.contains(&language) {
            out.push(language);
        }
    }
    out
}

/// EXP-1859 (`multiMatch`): which of `listeners` (each its phrase and its
/// language's transcript) a check heard: every one is matched by the
/// check's rule (`first` = [`wake_match`], else [`slide_match`]); the fewest
/// edits win, then the phrase with the most letters; a tie on both wakes
/// nobody. `Some((index, rest))` = the winner and what follows its phrase.
pub fn multi_match(listeners: &[(&str, &str)], first: bool) -> Option<(usize, String)> {
    let letters = |phrase: &str| wake_words(phrase).iter().map(|word| word.folded.chars().count()).sum::<usize>();
    let mut best: Option<(usize, usize, usize, String)> = None;
    let mut tied = false;
    for (index, (phrase, heard)) in listeners.iter().enumerate() {
        let matched = if first { wake_match_edits(phrase, heard) } else { slide_match_edits(phrase, heard) };
        let Some((edits, rest)) = matched else { continue };
        let size = letters(phrase);
        match best.as_ref() {
            Some((_, best_edits, best_size, _)) if (*best_edits, std::cmp::Reverse(*best_size)) < (edits, std::cmp::Reverse(size)) => {}
            Some((_, best_edits, best_size, _)) if (*best_edits, *best_size) == (edits, size) => tied = true,
            _ => {
                best = Some((index, edits, size, rest));
                tied = false;
            }
        }
    }
    best.filter(|_| !tied).map(|(index, _, _, rest)| (index, rest))
}

/// EXP-1424: a MODEL's turn (talk-listen.json `wake.detector.modelMatch`):
/// the model heard the phrase, so the transcript loses its closest opening
/// (starting at any of the first [`SLIDE_SKIP_WORDS`] + 1 words, the
/// closest wins, the earliest on a tie) within floor(letters ×
/// [`MODEL_EDIT_RATIO`]) edits; else it stays whole.
pub fn model_match(phrase: &str, heard: &str) -> Option<String> {
    let letters: usize = wake_words(phrase).iter().map(|word| word.folded.chars().count()).sum();
    let words = wake_words(heard);
    let mut best: Option<(usize, usize)> = None;
    for skip in 0..=SLIDE_SKIP_WORDS.min(words.len()) {
        let Some(word) = words.get(skip) else { break };
        let Some((count, distance)) = wake_distance(phrase, &heard[word.start..]) else { continue };
        if best.is_none_or(|(_, best)| distance < best) {
            best = Some((skip + count, distance));
        }
    }
    let (count, distance) = best?;
    (distance <= (letters as f32 * MODEL_EDIT_RATIO).floor() as usize).then(|| rest_after(heard, count))
}

/// What woke a turn, so what its transcript loses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opening {
    /// A segment's first check ([`wake_match`]).
    First,
    /// S-3: a later check ([`slide_match`] too).
    Later,
    /// EXP-1424: a model ([`model_match`]).
    Model,
}

/// `heard` without the phrase that woke the turn (unchanged when it no
/// longer opens with it).
pub fn strip(phrase: &str, opening: Opening, heard: &str) -> String {
    let stripped = match opening {
        Opening::First => wake_match(phrase, heard),
        Opening::Later => wake_match(phrase, heard).or_else(|| slide_match(phrase, heard)),
        Opening::Model => model_match(phrase, heard),
    };
    stripped.unwrap_or_else(|| heard.to_string())
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
    /// The segment's padded start (a segment that began while the voice
    /// played is never checked, however late the check).
    pub segment: usize,
    /// S-3: a later check of the segment ([`slide_match`]), not its first;
    /// EXP-1424: or a model's detection (never transcribed to match).
    pub opening: Opening,
}

impl Check {
    /// Whether `heard` (this check's transcript) is the phrase.
    pub fn matches(&self, phrase: &str, heard: &str) -> bool {
        match self.opening {
            Opening::First => wake_match(phrase, heard).is_some(),
            Opening::Later => slide_match(phrase, heard).is_some(),
            Opening::Model => true,
        }
    }
}

/// Speech segments into wake checks: the first when the open segment
/// reaches its window, or when a shorter one closes; S-3: then again every
/// [`WINDOW_HOP_MS`] while it stays open (its last window), and over its
/// tail when it closes with at least half a hop unchecked, at most
/// [`MAX_CHECKS_PER_SEGMENT`]. Also keeps the audio and scores the next
/// check, and a match's seed, need (the open segment, else only the padding
/// a segment could reach back into).
#[derive(Debug, Clone)]
pub struct WakeScanner {
    /// The window in frames.
    window: usize,
    segmenter: Segmenter,
    /// The checks of the open segment so far.
    checks: usize,
    /// Where the open segment's last check ended.
    checked_to: usize,
    /// The first frame kept in `audio` / `scores`.
    base: usize,
    audio: Vec<f32>,
    scores: Vec<f32>,
    /// Frames scored while the voice played: a segment starting before
    /// this is the reply's and never checked.
    played: usize,
    /// EXP-1424: frames kept back even in silence (a model's seed reaches
    /// back [`MODEL_HISTORY_MS`], past a segment that already closed).
    history: usize,
    /// #NEW-126: frames since Silero last scored one at [`MODEL_GATE`] or
    /// over (0 at the start: nothing is known quiet yet).
    unheard: usize,
}

impl WakeScanner {
    /// A scanner checking the first `window_ms` of each segment.
    pub fn new(window_ms: usize) -> Self {
        WakeScanner {
            window: frames(window_ms),
            segmenter: Segmenter::default(),
            checks: 0,
            checked_to: 0,
            base: 0,
            audio: Vec::new(),
            scores: Vec::new(),
            played: 0,
            history: frames(crate::listen::PAD_MS),
            unheard: 0,
        }
    }

    /// EXP-1424: a scanner for a model ([`SetDetector`]): it also keeps the last
    /// [`MODEL_HISTORY_MS`].
    pub fn for_model() -> Self {
        WakeScanner { history: frames(MODEL_HISTORY_MS), ..WakeScanner::new(WINDOW_MIN_MS) }
    }

    /// EXP-1859: this scanner also keeps a model's history (a listener of
    /// the set is heard by one).
    pub fn with_model_history(mut self) -> Self {
        self.history = self.history.max(frames(MODEL_HISTORY_MS));
        self
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
        self.unheard = if probability >= MODEL_GATE { 0 } else { self.unheard.saturating_add(1) };
        self.scores.push(probability);
        let closed = self.segmenter.push(probability);
        let hop = frames(WINDOW_HOP_MS);
        let last = |start: usize, end: usize| Check { start: end.saturating_sub(self.window).max(start), end, segment: start, opening: Opening::Later };
        let check = if let Some(segment) = closed {
            // The segment that just closed (a cut one included): its first
            // check, or its unchecked tail.
            let checks = std::mem::take(&mut self.checks);
            if checks == 0 {
                Some(Check {
                    start: segment.start,
                    end: segment.end.min(segment.start + self.window),
                    segment: segment.start,
                    opening: Opening::First,
                })
            } else {
                (checks < MAX_CHECKS_PER_SEGMENT && segment.end >= self.checked_to + frames(WINDOW_HOP_MS / 2))
                    .then(|| last(segment.start, segment.end))
            }
        } else if let Some(start) = self.segmenter.open_start() {
            let check = if self.checks == 0 {
                (self.segmenter.frames() - start >= self.window)
                    .then_some(Check { start, end: start + self.window, segment: start, opening: Opening::First })
            } else {
                // Grown = its speech (padded) reached that far: a pause
                // that may still close it adds nothing.
                let end = self.segmenter.open_end();
                (self.checks < MAX_CHECKS_PER_SEGMENT && end >= self.checked_to + hop).then(|| last(start, end))
            };
            if let Some(check) = check {
                self.checks += 1;
                self.checked_to = check.end;
            }
            check
        } else {
            self.checks = 0;
            None
        };
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

    /// #NEW-126: the room is quiet: no frame reached [`MODEL_GATE`] for
    /// [`MODEL_GATE_HOLD_MS`], so a model has nothing to score.
    pub fn quiet(&self) -> bool {
        self.unheard >= frames(MODEL_GATE_HOLD_MS)
    }

    /// Drop what no check or seed can reach any more.
    fn trim(&mut self) {
        let back = self.segmenter.frames().saturating_sub(self.history);
        let keep = self.segmenter.open_start().map_or(back, |start| start.min(back));
        if keep > self.base {
            let drop = keep - self.base;
            self.audio.drain(..(drop * FRAME).min(self.audio.len()));
            self.scores.drain(..drop.min(self.scores.len()));
            self.base = keep;
        }
    }
}

/// A wake: the check that matched, the scores of the capture's frames after
/// it (the turn's utterance takes them), and the listener that won.
pub type Woken = (Check, Vec<f32>, usize);

/// EXP-1424: how the scan hears the phrases: [`TranscriptMatcher`] (the
/// rules above) or a model of its own ([`SetDetector`]), picked per phrase
/// ([`detector_kind`]).
pub trait WakeDetector {
    /// One capture, already in `scanner` and scored (`probabilities`, one
    /// per frame): a match comes back as [`Woken`]. `playing` = the capture
    /// is the voice's own (or the cooldown's): a segment that starts in it
    /// never wakes. EXP-1859: `listeners` = every agent listened for.
    fn hear(
        &mut self,
        scanner: &mut WakeScanner,
        samples: &[f32],
        probabilities: &[f32],
        transcriber: &mut dyn Transcriber,
        listeners: &[Listener],
        playing: bool,
    ) -> Result<Option<Woken>, String>;

    fn kind(&self) -> DetectorKind;
}

/// Which detector hears a phrase (talk-listen.json `wake.detector.pick`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectorKind {
    Transcript,
    Model,
}

/// `phrase`'s detector among `models`: a model when one is published for
/// it, else the transcript matcher (the files still have to be ready: until
/// then the listener keeps the matcher).
pub fn detector_kind(models: &[etalk_wakeword::models::Model], phrase: &str) -> DetectorKind {
    match etalk_wakeword::models::model_in(models, phrase) {
        Some(_) => DetectorKind::Model,
        None => DetectorKind::Transcript,
    }
}

/// EXP-1859 (`wake.multi`): `check`'s window through the matcher for the
/// listeners `heard_here` picks: ONE transcript per language among them
/// ([`passes`]), then [`multi_match`]. `Some(index)` = the listener that won.
fn check_winner(
    pcm: &[f32],
    opening: Opening,
    listeners: &[Listener],
    heard_here: &dyn Fn(usize) -> bool,
    transcriber: &mut dyn Transcriber,
) -> Result<Option<usize>, String> {
    let picked: Vec<usize> = (0..listeners.len()).filter(|index| heard_here(*index)).collect();
    let mut heard: Vec<(&'static str, String)> = Vec::new();
    for language in passes(picked.iter().map(|index| listeners[*index].language)) {
        heard.push((language, transcriber.transcribe(pcm, language)?));
    }
    let transcript = |language: &str| heard.iter().find(|(spoken, _)| *spoken == language).map_or("", |(_, text)| text.as_str());
    let pairs: Vec<(&str, &str)> = picked.iter().map(|index| (listeners[*index].phrase.as_str(), transcript(listeners[*index].language))).collect();
    Ok(multi_match(&pairs, opening == Opening::First).map(|(winner, _)| picked[winner]))
}

/// The scanner's due checks over one capture, matched for the listeners
/// `heard_here` picks (none = the scanner is only fed).
fn hear_transcripts(
    scanner: &mut WakeScanner,
    probabilities: &[f32],
    transcriber: &mut dyn Transcriber,
    listeners: &[Listener],
    heard_here: &dyn Fn(usize) -> bool,
    playing: bool,
) -> Result<Option<Woken>, String> {
    let any = (0..listeners.len()).any(heard_here);
    for (index, probability) in probabilities.iter().enumerate() {
        let check = scanner.push(*probability);
        if playing {
            scanner.played = scanner.frames();
        }
        let Some(check) = check else { continue };
        if !any || check.segment < scanner.played {
            continue;
        }
        if let Some(winner) = check_winner(scanner.pcm(check), check.opening, listeners, heard_here, transcriber)? {
            return Ok(Some((check, probabilities[index + 1..].to_vec(), winner)));
        }
    }
    Ok(None)
}

/// #NEW-56: transcribe the checks the scanner makes due and match each
/// (EXP-1859: against every listener).
pub struct TranscriptMatcher;

impl WakeDetector for TranscriptMatcher {
    fn hear(
        &mut self,
        scanner: &mut WakeScanner,
        _samples: &[f32],
        probabilities: &[f32],
        transcriber: &mut dyn Transcriber,
        listeners: &[Listener],
        playing: bool,
    ) -> Result<Option<Woken>, String> {
        hear_transcripts(scanner, probabilities, transcriber, listeners, &|_| true, playing)
    }

    fn kind(&self) -> DetectorKind {
        DetectorKind::Transcript
    }
}

/// EXP-1424: a phrase's model scores the frames; Silero still segments
/// (the turn needs its scores), nothing is transcribed for it while
/// scanning. #NEW-126: in a quiet room ([`WakeScanner::quiet`]) the models
/// rest ([`etalk_wakeword::Detector::rest`]): Silero alone listens, and the
/// first frame it hears something in is scored as if they had never rested.
/// A detection is a [`Check`] from up to [`MODEL_HISTORY_MS`] before it
/// (never before its segment's padded start) to the detection; its segment
/// is the open one (else the check's start), so the echo and cooldown rules
/// hold. EXP-1859: `models` = each listener (its index) that has a model
/// ready; the other listeners keep the matcher, in the same scan.
#[derive(Default)]
pub struct SetDetector {
    pub models: Vec<(usize, etalk_wakeword::Detector)>,
}

impl SetDetector {
    /// Listener `index` is heard by `model`.
    pub fn model(index: usize, model: etalk_wakeword::Detector) -> Self {
        SetDetector { models: vec![(index, model)] }
    }
}

impl WakeDetector for SetDetector {
    fn hear(
        &mut self,
        scanner: &mut WakeScanner,
        samples: &[f32],
        probabilities: &[f32],
        transcriber: &mut dyn Transcriber,
        listeners: &[Listener],
        playing: bool,
    ) -> Result<Option<Woken>, String> {
        let modelled: Vec<usize> = self.models.iter().map(|(index, _)| *index).collect();
        let matched = hear_transcripts(scanner, probabilities, transcriber, listeners, &|index| !modelled.contains(&index), playing)?;
        if scanner.quiet() {
            for (_, model) in &mut self.models {
                model.rest(samples);
            }
            return Ok(matched);
        }
        // Always fed (their history stays whole); a detection while playing
        // is dropped below.
        let mut detected: Option<(usize, usize)> = None;
        for (index, model) in &mut self.models {
            if let Some(end) = model.detect(samples).map_err(|err| err.to_string())? {
                detected.get_or_insert((*index, end));
            }
        }
        if matched.is_some() {
            return Ok(matched);
        }
        let Some((winner, end)) = detected else { return Ok(None) };
        let end = (end / FRAME).min(scanner.frames());
        let open = scanner.segmenter.open_start();
        let start = end.saturating_sub(frames(MODEL_HISTORY_MS)).max(open.unwrap_or(0)).max(scanner.base);
        let segment = open.unwrap_or(start);
        if playing || segment < scanner.played {
            return Ok(None);
        }
        Ok(Some((Check { start, end, segment, opening: Opening::Model }, Vec::new(), winner)))
    }

    fn kind(&self) -> DetectorKind {
        if self.models.is_empty() { DetectorKind::Transcript } else { DetectorKind::Model }
    }
}

/// One capture through the scan: score it and let `detector` hear it. A
/// match comes back with the scores of the capture's frames after it (the
/// turn's utterance takes them). `playing` = a reply is being spoken (or
/// just was, [`ECHO_TAIL_MS`]): the capture is the voice's own, and a
/// segment that starts in it is never checked, so the agent can't wake
/// itself.
#[allow(clippy::too_many_arguments)]
pub fn scan(
    scanner: &mut WakeScanner,
    detector: &mut dyn WakeDetector,
    vad: &mut dyn Vad,
    transcriber: &mut dyn Transcriber,
    samples: &[f32],
    listeners: &[Listener],
    playing: bool,
) -> Result<Option<Woken>, String> {
    scanner.samples(samples);
    let probabilities = vad.push(samples)?;
    detector.hear(scanner, samples, &probabilities, transcriber, listeners, playing)
}

/// A wake's own sound (talk-listen.json `wake.multi` `tone`; owner
/// 2026-11-16: a short tone on every wake).
pub const TONE_NOTES_HZ: [f32; 2] = [660.0, 990.0];
pub const TONE_NOTE_MS: usize = 70;
pub const TONE_GAP_MS: usize = 30;
pub const TONE_FADE_MS: usize = 8;
pub const TONE_GAIN: f32 = 0.25;

/// EXP-1859: the wake tone at `rate`: the notes in order, each
/// [`TONE_NOTE_MS`] with [`TONE_FADE_MS`] ramps, [`TONE_GAP_MS`] apart.
pub fn wake_tone(rate: u32) -> Vec<f32> {
    let at = |ms: usize| rate as usize * ms / 1000;
    let (note, gap, fade) = (at(TONE_NOTE_MS), at(TONE_GAP_MS), at(TONE_FADE_MS).max(1));
    let mut out = Vec::with_capacity(TONE_NOTES_HZ.len() * (note + gap));
    for (index, hz) in TONE_NOTES_HZ.iter().enumerate() {
        if index > 0 {
            out.extend(std::iter::repeat_n(0.0, gap));
        }
        out.extend((0..note).map(|sample| {
            let envelope = (sample.min(note - 1 - sample) as f32 / fade as f32).min(1.0);
            TONE_GAIN * envelope * (std::f32::consts::TAU * hz * sample as f32 / rate as f32).sin()
        }));
    }
    out
}

/// A match's turn: the segment from the check's start seeds it, the
/// capture's later scores follow.
pub fn turn(
    id: &str,
    listeners: &[Listener],
    scanner: &WakeScanner,
    (check, rest, winner): Woken,
    transcriber: &mut dyn Transcriber,
) -> Result<Utterance, (etalk::ListenFailure, String)> {
    let (audio, scores) = scanner.seed(check);
    // EXP-1859: the turn is the winner's: its phrase is dropped, its
    // language heard.
    let listener = &listeners[winner];
    let mut utterance = Utterance::woken(id, listener.language, listener.phrase.as_str(), check.opening, &audio, &scores, transcriber)?;
    utterance.feed_scored(&[], &rest, transcriber, &mut |_| {})?;
    Ok(utterance)
}

/// #NEW-86: whether a segment heard `since_turn_ms` after the last wake turn
/// ended (None = no turn yet) falls in the cooldown, so it is never checked.
pub fn cooling(since_turn_ms: Option<usize>) -> bool {
    since_turn_ms.is_some_and(|ms| ms < COOLDOWN_MS)
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

    const FIXTURE: &str = include_str!("../../../../../packages/etalk/fixtures/talk-listen.json");

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
        assert_eq!(c["cooldownMs"], COOLDOWN_MS);
        assert_eq!(c["windowHopMs"], WINDOW_HOP_MS);
        assert_eq!(c["maxChecksPerSegment"], MAX_CHECKS_PER_SEGMENT);
        assert_eq!(c["slideEditRatio"].as_f64().unwrap() as f32, SLIDE_EDIT_RATIO);
        assert_eq!(c["slideSkipWords"], SLIDE_SKIP_WORDS);
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
    fn slide_matches_follow_the_fixture() {
        let cases = wake()["slideMatch"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let got = slide_match(case["phrase"].as_str().unwrap(), case["heard"].as_str().unwrap());
            let expected = case["rest"].as_str().map(str::to_string);
            assert_eq!(got, expected, "{}", case["name"]);
        }
    }

    /// EXP-1859: `wake.multi`.
    #[test]
    fn several_listeners_follow_the_fixture() {
        let multi = &wake()["multi"];
        assert_eq!(multi["constants"]["maxListeners"], MAX_LISTENERS);
        for case in multi["multiWindow"].as_array().unwrap() {
            let phrases: Vec<&str> = case["phrases"].as_array().unwrap().iter().map(|phrase| phrase.as_str().unwrap()).collect();
            assert_eq!(multi_window_ms(phrases), case["windowMs"].as_u64().unwrap() as usize, "{case}");
        }
        for case in multi["passes"].as_array().unwrap() {
            let languages: Vec<&str> = case["languages"].as_array().unwrap().iter().map(|language| language.as_str().unwrap()).collect();
            let expected: Vec<String> = serde_json::from_value(case["passes"].clone()).unwrap();
            assert_eq!(passes(languages), expected, "{case}");
        }
        let cases = multi["multiMatch"].as_array().unwrap();
        assert!(cases.len() >= 9);
        for case in cases {
            let heard = case["heard"].as_str().unwrap();
            let listeners = case["listeners"].as_array().unwrap();
            let pairs: Vec<(&str, &str)> = listeners.iter().map(|listener| (listener["phrase"].as_str().unwrap(), heard)).collect();
            let got = multi_match(&pairs, case["first"].as_bool().unwrap()).map(|(index, rest)| (listeners[index]["agent"].as_str().unwrap().to_string(), rest));
            let expected = case["expected"].as_object().map(|expected| (expected["agent"].as_str().unwrap().to_string(), expected["rest"].as_str().unwrap().to_string()));
            assert_eq!(got, expected, "{}", case["name"]);
        }
    }

    /// EXP-1859: the wake tone is the fixture's, short and soft.
    #[test]
    fn the_wake_tone_follows_the_fixture() {
        let tone = &wake()["multi"]["tone"];
        let notes: Vec<f32> = serde_json::from_value(tone["notesHz"].clone()).unwrap();
        assert_eq!(notes, TONE_NOTES_HZ);
        assert_eq!(tone["noteMs"], TONE_NOTE_MS);
        assert_eq!(tone["gapMs"], TONE_GAP_MS);
        assert_eq!(tone["fadeMs"], TONE_FADE_MS);
        assert_eq!(tone["gain"].as_f64().unwrap() as f32, TONE_GAIN);
        let pcm = wake_tone(24_000);
        assert_eq!(pcm.len(), 24 * (2 * TONE_NOTE_MS + TONE_GAP_MS));
        assert!(pcm.iter().all(|sample| sample.abs() <= TONE_GAIN));
        assert!(pcm.iter().any(|sample| sample.abs() > TONE_GAIN * 0.9));
        // Ramps: every note starts and ends at silence, the gap is silent.
        let note = 24 * TONE_NOTE_MS;
        assert_eq!((pcm[0], pcm[note - 1].abs() < 0.01, pcm[note + 10]), (0.0, true, 0.0));
    }

    /// EXP-1859: one capture, several listeners: ONE transcript per
    /// language, the winner's turn loses ITS phrase.
    #[test]
    fn several_listeners_share_one_transcript_and_the_winner_takes_the_turn() {
        let listeners = [Listener::new("a-bro", "Hey Assistant", "en"), Listener::new("a-tri", "Hey Triage", "en"), Listener::new("a-ops", "Hey Ops", "en")];
        let audio = [tone(0.3, 0.0), tone(3.0, 0.5), tone(1.5, 0.0)].concat();
        let mut vad = Loud(Vec::new());
        let mut engines = Says("Hey Triage, what's new?", 0);
        let mut scanner = WakeScanner::new(multi_window_ms(listeners.iter().map(|listener| listener.phrase.as_str())));
        let (mut turn, mut answer, mut woke, mut scanned) = (None::<Utterance>, None, None, 0);
        for chunk in audio.chunks(320) {
            if let Some(utterance) = turn.as_mut() {
                utterance.feed(chunk, &mut vad, &mut engines, &mut |_| {}).unwrap();
                if turn_ended(utterance) {
                    turn.take().unwrap().finish(&mut engines, &mut |event| answer = Some(event)).unwrap();
                    break;
                }
                continue;
            }
            if let Some(matched) = scan(&mut scanner, &mut TranscriptMatcher, &mut vad, &mut engines, chunk, &listeners, false).unwrap() {
                scanned = engines.1;
                woke = Some(listeners[matched.2].agent_id.clone());
                turn = Some(super::turn("wake-1", &listeners, &scanner, matched, &mut engines).unwrap());
            }
        }
        assert_eq!(woke.as_deref(), Some("a-tri"));
        assert_eq!(scanned, 1, "three phrases of one language cost one transcript");
        assert_eq!(answer, Some(etalk::Event::Heard { id: "wake-1".into(), text: "What's new?".into() }));
    }

    /// EXP-1859: a second language costs one more transcript per check.
    #[test]
    fn a_second_language_is_one_more_pass() {
        let listeners = [Listener::new("a-bro", "Hey Assistant", "en"), Listener::new("a-stu", "Hey Studio", "de"), Listener::new("a-ops", "Hey Ops", "fr")];
        let mut engines = Says("Something else entirely.", 0);
        let winner = check_winner(&[0.0; FRAME], Opening::First, &listeners, &|_| true, &mut engines).unwrap();
        assert_eq!((winner, engines.1), (None, 2));
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
                        assert_eq!(check.opening == Opening::Later, checks.last().is_some_and(|last: &[usize; 2]| last[0] >= check.segment), "{}", case["name"]);
                        checks.push([check.start, check.end]);
                    }
                }
            }
            let expected: Vec<[usize; 2]> = serde_json::from_value(case["checks"].clone()).unwrap();
            assert_eq!(checks, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn the_cooldown_follows_the_fixture() {
        for case in wake()["cooldown"].as_array().unwrap() {
            let since = case["msSinceTurn"].as_u64().map(|ms| ms as usize);
            assert_eq!(!cooling(since), case["checked"].as_bool().unwrap(), "{}", case["name"]);
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

    fn detector() -> serde_json::Value {
        wake()["detector"].clone()
    }

    #[test]
    fn the_model_constants_are_the_fixture() {
        let c = &detector()["constants"];
        assert_eq!(c["frameMs"], etalk_wakeword::FRAME_SAMPLES * 1000 / etalk_wakeword::RATE);
        assert_eq!(c["threshold"].as_f64().unwrap() as f32, etalk_wakeword::models::THRESHOLD);
        assert_eq!(c["patienceFrames"], etalk_wakeword::models::PATIENCE);
        assert_eq!(c["debounceFrames"], etalk_wakeword::models::DEBOUNCE);
        assert_eq!(c["modelHistoryMs"], MODEL_HISTORY_MS);
        assert_eq!(c["modelEditRatio"].as_f64().unwrap() as f32, MODEL_EDIT_RATIO);
        assert_eq!(c["gate"].as_f64().unwrap() as f32, MODEL_GATE);
        assert_eq!(c["gateHoldMs"], MODEL_GATE_HOLD_MS);
        assert_eq!(MODEL_GATE_HOLD_MS, etalk_wakeword::CLASSIFIER_WINDOW * etalk_wakeword::FRAME_SAMPLES * 1000 / etalk_wakeword::RATE, "the classifier's window");
        for model in etalk_wakeword::models::MODELS {
            assert!(model.threshold > 0.0 && model.threshold < 1.0, "{}: its own measured threshold", model.id);
        }
    }

    #[test]
    fn detectors_are_picked_like_the_fixture() {
        use etalk_wakeword::models::{Model, Pin, HEY_SPEZL};
        let published = |key: &'static str| Model {
            phrases: Box::leak(Box::new([key])),
            classifier: Pin { url: "https://example.invalid/m.onnx", sha256: "00", ..HEY_SPEZL.classifier },
            ..HEY_SPEZL
        };
        for case in detector()["pick"].as_array().unwrap() {
            let models: Vec<Model> =
                case["models"].as_array().unwrap().iter().map(|key| published(key.as_str().unwrap().to_string().leak())).collect();
            let expected = match case["detector"].as_str().unwrap() {
                "model" => DetectorKind::Model,
                _ => DetectorKind::Transcript,
            };
            assert_eq!(detector_kind(&models, case["phrase"].as_str().unwrap()), expected, "{}", case["name"]);
        }
    }

    #[test]
    fn triggers_fire_like_the_fixture() {
        let c = &detector()["constants"];
        for case in detector()["trigger"].as_array().unwrap() {
            let mut trigger = etalk_wakeword::Trigger::new(
                c["threshold"].as_f64().unwrap() as f32,
                c["patienceFrames"].as_u64().unwrap() as usize,
                c["debounceFrames"].as_u64().unwrap() as usize,
            );
            let fires: Vec<usize> = case["scores"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter_map(|(frame, score)| trigger.push(score.as_f64().unwrap() as f32).then_some(frame))
                .collect();
            let expected: Vec<usize> = serde_json::from_value(case["fires"].clone()).unwrap();
            assert_eq!(fires, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn model_matches_follow_the_fixture() {
        for case in detector()["modelMatch"].as_array().unwrap() {
            let (phrase, heard) = (case["phrase"].as_str().unwrap(), case["heard"].as_str().unwrap());
            let expected = case["rest"].as_str().map(str::to_string);
            assert_eq!(model_match(phrase, heard), expected, "{}", case["name"]);
            assert_eq!(strip(phrase, Opening::Model, heard), expected.unwrap_or(heard.to_string()), "{}", case["name"]);
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
    fn wake_on(audio: &[f32], says: &'static str) -> (Option<etalk::Event>, usize) {
        wake_on_playing(audio, says, 0)
    }

    /// [`wake_on`] while a reply plays over the first `played` samples.
    fn wake_on_playing(audio: &[f32], says: &'static str, played: usize) -> (Option<etalk::Event>, usize) {
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
            let listeners = [Listener::new("", "Hey Studio", "en")];
            if let Some(matched) = scan(&mut scanner, &mut TranscriptMatcher, &mut vad, &mut engines, chunk, &listeners, playing).unwrap() {
                turn = Some(super::turn("wake-1", &listeners, &scanner, matched, &mut engines).unwrap());
            }
        }
        (answer, engines.1)
    }

    #[test]
    fn a_phrase_and_a_command_are_one_turn_without_the_phrase() {
        let audio = [tone(0.5, 0.0), tone(1.2, 0.3), tone(1.5, 0.0)].concat();
        let (answer, transcriptions) = wake_on(&audio, "Hey Studio, move NEW-78 to done.");
        assert_eq!(answer, Some(etalk::Event::Heard { id: "wake-1".into(), text: "Move NEW-78 to done.".into() }));
        assert_eq!(transcriptions, 2, "the check, then the turn's segment");
    }

    #[test]
    fn a_phrase_alone_ends_empty_after_the_wait() {
        let audio = [tone(0.3, 0.0), tone(0.8, 0.3), tone(5.0, 0.0)].concat();
        let (answer, _) = wake_on(&audio, "Hey Studio.");
        assert_eq!(answer, Some(etalk::Event::Empty { id: "wake-1".into() }));
    }

    #[test]
    fn other_speech_is_transcribed_once_per_segment_and_never_wakes() {
        let audio = [tone(0.3, 0.0), tone(0.8, 0.3), tone(0.6, 0.0), tone(3.0, 0.3), tone(0.6, 0.0), tone(10.0, 0.0)].concat();
        let (answer, transcriptions) = wake_on(&audio, "They studied it.");
        assert_eq!(answer, None);
        // S-3: the short segment once; the 3 s one at its window, a hop
        // later and over its tail (18 frames unchecked >= half a hop).
        assert_eq!(transcriptions, 4, "the checks of each segment; silence costs none");
    }

    /// Answers each transcription with the next line (the last repeats).
    struct Script(Vec<&'static str>, usize);

    impl crate::listen::Transcriber for Script {
        fn transcribe(&mut self, _pcm: &[f32], _language: &str) -> Result<String, String> {
            self.1 += 1;
            Ok(self.0[(self.1 - 1).min(self.0.len() - 1)].to_string())
        }
        fn partials(&self) -> bool {
            false
        }
    }

    /// S-3: the phrase said into a segment that was already open (talk
    /// going on) wakes at a later check, and the turn loses the fragments
    /// before it with the phrase.
    #[test]
    fn a_phrase_inside_a_long_segment_wakes_at_a_later_check() {
        let audio = [tone(0.3, 0.0), tone(3.0, 0.3), tone(1.5, 0.0)].concat();
        let mut vad = Loud(Vec::new());
        let mut engines = Script(vec!["We talked about the release", "the release and hey Studio, move it."], 0);
        let mut scanner = WakeScanner::new(window_ms("Hey Studio"));
        let mut turn: Option<Utterance> = None;
        let mut answer = None;
        let mut woke_at = None;
        for chunk in audio.chunks(320) {
            if let Some(utterance) = turn.as_mut() {
                utterance.feed(chunk, &mut vad, &mut engines, &mut |_| {}).unwrap();
                if turn_ended(utterance) {
                    turn.take().unwrap().finish(&mut engines, &mut |event| answer = Some(event)).unwrap();
                    break;
                }
                continue;
            }
            let listeners = [Listener::new("", "Hey Studio", "en")];
            if let Some(matched) = scan(&mut scanner, &mut TranscriptMatcher, &mut vad, &mut engines, chunk, &listeners, false).unwrap() {
                woke_at = Some(matched.0);
                turn = Some(super::turn("wake-1", &listeners, &scanner, matched, &mut engines).unwrap());
            }
        }
        let check = woke_at.expect("a later check woke it");
        assert!(check.opening == Opening::Later && check.start > check.segment, "{check:?}");
        assert_eq!(answer, Some(etalk::Event::Heard { id: "wake-1".into(), text: "Move it.".into() }));
        // The first window alone would never have woken it.
        assert_eq!(wake_match("Hey Studio", "the release and hey Studio, move it."), None);
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
        assert_eq!(answer, Some(etalk::Event::Heard { id: "wake-1".into(), text: "Move NEW-78 to done.".into() }), "the person's phrase after it");
    }

    /// Speech wherever a frame is louder than a whisper (real recordings).
    struct Energy(Vec<f32>);

    impl crate::listen::Vad for Energy {
        fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
            self.0.extend_from_slice(samples);
            let whole = self.0.len() / FRAME * FRAME;
            let rms = |frame: &[f32]| (frame.iter().map(|x| x * x).sum::<f32>() / frame.len() as f32).sqrt();
            let out = self.0[..whole].chunks(FRAME).map(|frame| if rms(frame) > 0.01 { 0.9 } else { 0.05 }).collect();
            self.0.drain(..whole);
            Ok(out)
        }
    }

    /// EXP-1424: a phrase with a model wakes without a transcription while
    /// scanning, and the turn loses the phrase (livekit-wakeword's
    /// `hey_livekit` and its recording); during the voice's own playback it
    /// never wakes.
    /// What the turn answered, the transcriptions spent while scanning, and
    /// the check that woke it.
    fn model_wake(played: usize) -> (Option<etalk::Event>, usize, Option<Check>) {
        model_wake_after(1.0, played)
    }

    /// [`model_wake`] with `quiet` seconds of silence before the phrase.
    fn model_wake_after(quiet: f32, played: usize) -> (Option<etalk::Event>, usize, Option<Check>) {
        let fixtures = crate::wakeword_fixtures::fixtures().join("tests/fixtures");
        let (phrase, _) = crate::wakeword_fixtures::wav(&fixtures.join("positive.wav"));
        let audio = [tone(quiet, 0.0), phrase, tone(NO_COMMAND_MS as f32 / 1000.0 + 1.0, 0.0)].concat();
        let model = etalk_wakeword::Detector::new(
            crate::wakeword_fixtures::front_end(),
            crate::wakeword_fixtures::hey_livekit(),
            etalk_wakeword::Trigger::new(etalk_wakeword::models::THRESHOLD, etalk_wakeword::models::PATIENCE, etalk_wakeword::models::DEBOUNCE),
            1.0,
        );
        let mut detector = SetDetector::model(0, model);
        let listeners = [Listener::new("", "Hey LiveKit", "en")];
        let mut vad = Energy(Vec::new());
        let mut engines = Says("Hey LiveKit, move it.", 0);
        let mut scanner = WakeScanner::for_model();
        let (mut turn, mut answer, mut woke) = (None::<Utterance>, None, None);
        let mut scanned = None;
        for (index, chunk) in audio.chunks(320).enumerate() {
            if let Some(utterance) = turn.as_mut() {
                utterance.feed(chunk, &mut vad, &mut engines, &mut |_| {}).unwrap();
                if turn_ended(utterance) {
                    turn.take().unwrap().finish(&mut engines, &mut |event| answer = Some(event)).unwrap();
                    break;
                }
                continue;
            }
            let playing = index * 320 < played;
            if let Some(matched) = scan(&mut scanner, &mut detector, &mut vad, &mut engines, chunk, &listeners, playing).unwrap() {
                scanned = Some(engines.1);
                woke = Some(matched.0);
                turn = Some(super::turn("wake-1", &listeners, &scanner, matched, &mut engines).unwrap());
            }
        }
        (answer, scanned.unwrap_or(engines.1), woke)
    }

    #[test]
    fn a_model_wakes_without_transcribing_and_strips_the_phrase() {
        let (answer, scanned, check) = model_wake(0);
        let check = check.expect("the model heard the phrase");
        assert_eq!(check.opening, Opening::Model);
        assert!(check.start >= check.end.saturating_sub(frames(MODEL_HISTORY_MS)) && check.start < check.end, "{check:?}");
        assert_eq!(scanned, 0, "nothing transcribed while scanning");
        assert_eq!(answer, Some(etalk::Event::Heard { id: "wake-1".into(), text: "Move it.".into() }));
    }

    /// #NEW-126: in a quiet room the model rests (no frame scored), and the
    /// phrase after it wakes exactly as it does in a stream scored whole:
    /// the same check, as many frames later as the quiet lasted longer.
    #[test]
    fn a_model_that_rested_wakes_like_one_that_never_did() {
        let (answer, _, check) = model_wake(0);
        let check = check.expect("the model heard the phrase");
        // 8 s more: 250 Silero frames, 100 of the model's.
        let (rested_answer, scanned, rested) = model_wake_after(9.0, 0);
        let rested = rested.expect("the model heard the phrase after its rest");
        assert_eq!((rested.start, rested.end), (check.start + 250, check.end + 250));
        assert_eq!((rested_answer, scanned), (answer, 0));
    }

    /// #NEW-126: the gate, frame by frame, as the fixture has it.
    #[test]
    fn the_model_rests_like_the_fixture() {
        for case in detector()["gate"].as_array().unwrap() {
            let mut scanner = WakeScanner::for_model();
            let mut quiet = Vec::new();
            for run in case["frames"].as_array().unwrap() {
                for _ in 0..run["count"].as_u64().unwrap() {
                    scanner.samples(&[0.0; FRAME]);
                    scanner.push(run["probability"].as_f64().unwrap() as f32);
                }
                quiet.push(scanner.quiet());
            }
            let expected: Vec<bool> = case["frames"].as_array().unwrap().iter().map(|run| run["rests"].as_bool().unwrap()).collect();
            assert_eq!(quiet, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn a_model_never_wakes_on_the_voice() {
        let (answer, scanned, check) = model_wake(usize::MAX);
        assert_eq!((answer, scanned, check), (None, 0, None));
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
        assert_eq!(check, Check { start: 18, end: 18 + frames(2_000), segment: 18, opening: Opening::First });
        let (audio, scores) = scanner.seed(check);
        assert_eq!(scores.len(), 90 - 18);
        assert_eq!(audio.len(), scores.len() * FRAME);
        assert_eq!(audio[0], 18.0, "the padded start");
    }
}
