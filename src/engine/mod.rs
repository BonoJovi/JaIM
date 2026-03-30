/// JaIM Conversion Engine
///
/// Orchestrates the 3-stage conversion pipeline:
/// 1. Dictionary lookup + segmentation (fast, < 1ms)
/// 2. Grammar scoring (fast, < 1ms)
/// 3. LLM reranking (20-40ms, background pre-computation)
///
/// Flow: keystroke → romaji → kana → dictionary segment → grammar score
///       → LLM rerank → candidate list → user selects → commit

use crate::core::{
    dictionary::{Dictionary, DictionaryEntry, Segment},
    grammar::{GrammarEngine, GrammarToken},
    llm::LlmEngine,
    romaji::RomajiConverter,
    user_scorer::UserScorer,
};

/// Per-segment state during conversion
#[derive(Debug, Clone)]
pub struct SegmentState {
    /// Hiragana reading for this segment
    pub reading: String,
    /// Start position in kana (char offset)
    pub start: usize,
    /// Candidate surfaces (sorted by score)
    pub candidates: Vec<String>,
    /// Currently selected candidate index
    pub selected: usize,
    /// Whether the user explicitly changed the candidate for this segment
    pub user_selected: bool,
}

/// Active conversion state (set after Space is pressed)
#[derive(Debug, Clone)]
pub struct ConversionState {
    /// The original kana string
    pub kana: String,
    /// Per-segment conversion state
    pub segments: Vec<SegmentState>,
    /// Currently focused segment index
    pub focus: usize,
}

impl ConversionState {
    /// Get the composed text from all segments' selected candidates.
    pub fn composed_text(&self) -> String {
        self.segments
            .iter()
            .map(|seg| seg.candidates[seg.selected].as_str())
            .collect()
    }

    /// Get segment boundary info: Vec of (start_char, end_char) in composed text.
    pub fn segment_char_ranges(&self) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let mut pos = 0;
        for seg in &self.segments {
            let text = &seg.candidates[seg.selected];
            let len = text.chars().count();
            ranges.push((pos, pos + len));
            pos += len;
        }
        ranges
    }
}

pub struct ConversionEngine {
    romaji: RomajiConverter,
    dictionary: Dictionary,
    grammar: GrammarEngine,
    llm: LlmEngine,
    user_scorer: UserScorer,
    /// Path to persist user scores
    user_scores_path: Option<std::path::PathBuf>,
    /// Active conversion state (None when not converting)
    conversion: Option<ConversionState>,
}

impl ConversionEngine {
    pub fn new() -> Self {
        let scores_path = UserScorer::default_path().ok();
        let user_scorer = scores_path
            .as_ref()
            .and_then(|p| match UserScorer::load(p) {
                Ok(s) => Some(s),
                Err(e) => {
                    log::warn!("Failed to load user scores: {}", e);
                    None
                }
            })
            .unwrap_or_else(UserScorer::new);

        Self {
            romaji: RomajiConverter::new(),
            dictionary: Dictionary::new(),
            grammar: GrammarEngine::new(),
            llm: LlmEngine::new(),
            user_scorer,
            user_scores_path: scores_path,
            conversion: None,
        }
    }

    /// Process a key event from the IME framework.
    /// Returns the appropriate action for the UI layer.
    pub fn process_key(&mut self, key: char) -> EngineAction {
        if let Some(_kana) = self.romaji.process_key(key) {
            EngineAction::UpdatePreedit(self.preedit())
        } else {
            EngineAction::Buffering(self.preedit())
        }
    }

    /// Append a raw string directly to the preedit (e.g., punctuation).
    pub fn append_raw(&mut self, s: &str) {
        self.romaji.flush();
        self.romaji.append_raw(s);
    }

    /// Get the current preedit string (kana output + pending romaji buffer).
    pub fn preedit(&self) -> String {
        let mut preedit = self.romaji.output().to_string();
        let buf = self.romaji.buffer();
        if !buf.is_empty() {
            preedit.push_str(buf);
        }
        preedit
    }

    /// Start segment-based conversion (space key pressed).
    /// Returns the conversion state if successful.
    pub fn start_conversion(&mut self) -> Option<&ConversionState> {
        self.romaji.flush();
        let kana = self.romaji.output().to_string();
        if kana.is_empty() {
            return None;
        }

        let segments = self.dictionary.segment_with_boost(&kana, |reading, entries| {
            entries
                .iter()
                .map(|e| self.user_scorer.score(reading, &e.surface))
                .fold(0.0_f64, f64::max)
                * 10.0 // Scale boost to be significant vs segment cost
        });
        if segments.is_empty() {
            return None;
        }

        let segment_states = self.build_segment_states(&segments);
        self.conversion = Some(ConversionState {
            kana,
            segments: segment_states,
            focus: 0,
        });
        self.conversion.as_ref()
    }

    /// Get the current conversion state.
    pub fn conversion_state(&self) -> Option<&ConversionState> {
        self.conversion.as_ref()
    }

    /// Move focus to the next/previous segment. delta: +1 = right, -1 = left.
    pub fn move_focus(&mut self, delta: i32) -> Option<&ConversionState> {
        let state = self.conversion.as_mut()?;
        let len = state.segments.len();
        if len == 0 {
            return self.conversion.as_ref();
        }
        state.focus = if delta > 0 {
            (state.focus + 1) % len
        } else if state.focus == 0 {
            len - 1
        } else {
            state.focus - 1
        };
        self.conversion.as_ref()
    }

    /// Cycle the candidate for the focused segment. delta: +1 = next, -1 = previous.
    pub fn cycle_candidate(&mut self, delta: i32) -> Option<&ConversionState> {
        let state = self.conversion.as_mut()?;
        let seg = &mut state.segments[state.focus];
        let len = seg.candidates.len();
        if len == 0 {
            return self.conversion.as_ref();
        }
        seg.selected = if delta > 0 {
            (seg.selected + 1) % len
        } else if seg.selected == 0 {
            len - 1
        } else {
            seg.selected - 1
        };
        seg.user_selected = true;
        self.conversion.as_ref()
    }

    /// Resize the focused segment. delta: +1 = extend right, -1 = shrink right.
    /// Re-segments the affected regions and re-looks up candidates.
    pub fn resize_segment(&mut self, delta: i32) -> Option<&ConversionState> {
        let state = self.conversion.as_mut()?;
        let focus = state.focus;
        let seg_count = state.segments.len();

        if delta > 0 {
            // Extend: take one char from the next segment
            if focus + 1 >= seg_count {
                return self.conversion.as_ref();
            }
            let next_reading: Vec<char> = state.segments[focus + 1].reading.chars().collect();
            if next_reading.is_empty() {
                return self.conversion.as_ref();
            }
            // Move first char of next segment to current segment
            let ch = next_reading[0];
            state.segments[focus].reading.push(ch);
            let new_next: String = next_reading[1..].iter().collect();
            if new_next.is_empty() {
                state.segments.remove(focus + 1);
            } else {
                state.segments[focus + 1].reading = new_next;
                state.segments[focus + 1].start += 1;
            }
        } else {
            // Shrink: move last char of current segment to next segment
            let cur_reading: Vec<char> = state.segments[focus].reading.chars().collect();
            if cur_reading.len() <= 1 {
                return self.conversion.as_ref();
            }
            let last_ch = *cur_reading.last().unwrap();
            let new_cur: String = cur_reading[..cur_reading.len() - 1].iter().collect();
            state.segments[focus].reading = new_cur;

            if focus + 1 < state.segments.len() {
                let next = &mut state.segments[focus + 1];
                next.reading.insert(0, last_ch);
                next.start -= 1;
            } else {
                // Create a new segment after current
                let start = state.segments[focus].start
                    + state.segments[focus].reading.chars().count();
                state.segments.push(SegmentState {
                    reading: last_ch.to_string(),
                    start,
                    candidates: vec![last_ch.to_string()],
                    selected: 0,
                    user_selected: false,
                });
            }
        }

        // Re-lookup candidates for affected segments
        self.relookup_segment(focus);
        if focus + 1 < self.conversion.as_ref().unwrap().segments.len() {
            self.relookup_segment(focus + 1);
        }

        // Mark resized segments as user-selected
        if let Some(state) = self.conversion.as_mut() {
            state.segments[focus].user_selected = true;
            if focus + 1 < state.segments.len() {
                state.segments[focus + 1].user_selected = true;
            }
        }

        self.conversion.as_ref()
    }

    /// Set the focused segment's selected candidate to its hiragana reading (F6).
    pub fn convert_focused_to_hiragana(&mut self) -> Option<&ConversionState> {
        let state = self.conversion.as_mut()?;
        let seg = &mut state.segments[state.focus];
        if let Some(pos) = seg.candidates.iter().position(|c| c == &seg.reading) {
            seg.selected = pos;
        } else {
            seg.candidates.push(seg.reading.clone());
            seg.selected = seg.candidates.len() - 1;
        }
        seg.user_selected = true;
        self.conversion.as_ref()
    }

    /// Start a kana-form conversion (F6/F7/F8 outside conversion mode).
    /// Creates a single-segment conversion with hiragana, katakana, and half-width katakana
    /// as candidates, and selects the one matching the requested form.
    /// form: 0 = hiragana, 1 = katakana, 2 = half-width katakana
    pub fn start_kana_conversion(&mut self, form: usize) -> Option<&ConversionState> {
        self.romaji.flush();
        let kana = self.romaji.output().to_string();
        if kana.is_empty() {
            return None;
        }

        let katakana = crate::core::romaji::hiragana_to_katakana(&kana);
        let half_katakana = crate::core::romaji::hiragana_to_halfwidth_katakana(&kana);

        let mut candidates = vec![kana.clone(), katakana, half_katakana];
        // Deduplicate while preserving order
        let mut seen = std::collections::HashSet::new();
        candidates.retain(|c| seen.insert(c.clone()));

        let selected = form.min(candidates.len().saturating_sub(1));

        self.conversion = Some(ConversionState {
            kana: kana.clone(),
            segments: vec![SegmentState {
                reading: kana,
                start: 0,
                candidates,
                selected,
                user_selected: form != 0, // F7/F8 is an explicit choice
            }],
            focus: 0,
        });
        self.conversion.as_ref()
    }

    /// Convert the focused segment's reading to katakana and set it as the selected candidate.
    pub fn convert_focused_to_katakana(&mut self) -> Option<&ConversionState> {
        let state = self.conversion.as_mut()?;
        let seg = &mut state.segments[state.focus];
        let katakana = crate::core::romaji::hiragana_to_katakana(&seg.reading);
        if let Some(pos) = seg.candidates.iter().position(|c| c == &katakana) {
            seg.selected = pos;
        } else {
            seg.candidates.push(katakana);
            seg.selected = seg.candidates.len() - 1;
        }
        seg.user_selected = true;
        self.conversion.as_ref()
    }

    /// Clear conversion state (on commit or cancel).
    pub fn clear_conversion(&mut self) {
        self.conversion = None;
    }

    /// Trigger conversion (legacy interface for tests).
    /// Runs the full 3-stage pipeline: dictionary → grammar → LLM.
    pub fn convert(&mut self) -> Vec<ConversionCandidate> {
        self.romaji.flush();
        let kana = self.romaji.output().to_string();
        if kana.is_empty() {
            return Vec::new();
        }

        let segments = self.dictionary.segment(&kana);
        if segments.is_empty() {
            return Vec::new();
        }

        let candidates = self.build_candidates(&segments);

        let mut scored: Vec<ConversionCandidate> = candidates
            .into_iter()
            .map(|text| {
                let grammar_tokens = self.tokens_for_grammar(&text, &segments);
                let grammar_result = self.grammar.score(&grammar_tokens);
                let llm_score = self.llm.score_candidate(&text);
                let combined = grammar_result.score * 0.4 + llm_score * 0.6;
                ConversionCandidate {
                    text,
                    grammar_score: grammar_result.score,
                    llm_score,
                    score: combined,
                }
            })
            .collect();

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }

    /// Convert the focused segment's reading to half-width katakana.
    pub fn convert_focused_to_halfwidth_katakana(&mut self) -> Option<&ConversionState> {
        let state = self.conversion.as_mut()?;
        let seg = &mut state.segments[state.focus];
        let hw = crate::core::romaji::hiragana_to_halfwidth_katakana(&seg.reading);
        if let Some(pos) = seg.candidates.iter().position(|c| c == &hw) {
            seg.selected = pos;
        } else {
            seg.candidates.push(hw);
            seg.selected = seg.candidates.len() - 1;
        }
        seg.user_selected = true;
        self.conversion.as_ref()
    }

    /// Convert the current preedit to half-width katakana (F8, not in conversion mode).
    pub fn convert_to_halfwidth_katakana(&mut self) -> Option<String> {
        self.romaji.flush();
        let kana = self.romaji.output().to_string();
        if kana.is_empty() {
            return None;
        }
        Some(crate::core::romaji::hiragana_to_halfwidth_katakana(&kana))
    }

    /// Convert the current preedit to full-width katakana (F7).
    pub fn convert_to_katakana(&mut self) -> Option<String> {
        self.romaji.flush();
        let kana = self.romaji.output().to_string();
        if kana.is_empty() {
            return None;
        }
        Some(crate::core::romaji::hiragana_to_katakana(&kana))
    }

    /// Commit the selected candidate and update context.
    pub fn commit(&mut self, candidate: &str) -> String {
        self.llm.update_context(candidate);
        self.romaji.reset();
        candidate.to_string()
    }

    /// Commit the current conversion, recording user selections for learning.
    /// Returns the composed text if there was an active conversion.
    pub fn commit_conversion(&mut self) -> Option<String> {
        let state = self.conversion.take()?;
        let text = state.composed_text();

        // Record only segments where the user explicitly chose a candidate
        for seg in &state.segments {
            if seg.user_selected {
                let surface = &seg.candidates[seg.selected];
                self.user_scorer.record(&seg.reading, surface);
            }
        }

        // Persist scores
        if let Some(ref path) = self.user_scores_path {
            if let Err(e) = self.user_scorer.save(path) {
                log::warn!("Failed to save user scores: {}", e);
            }
        }

        self.llm.update_context(&text);
        self.romaji.reset();
        Some(text)
    }

    /// Delete the last character from the preedit (backspace).
    /// Returns true if something was deleted.
    pub fn delete_last(&mut self) -> bool {
        self.romaji.delete_last()
    }

    /// Reset the engine state (e.g., on focus change).
    pub fn reset(&mut self) {
        self.romaji.reset();
    }

    /// Build SegmentState list from dictionary Segments.
    /// Candidates are ordered by effective score (dictionary frequency + user learning).
    fn build_segment_states(&self, segments: &[Segment]) -> Vec<SegmentState> {
        segments
            .iter()
            .map(|seg| {
                let mut entries: Vec<&DictionaryEntry> = seg.candidates.iter().collect();
                entries.sort_by(|a, b| {
                    let score_a = self.effective_score(&seg.reading, a);
                    let score_b = self.effective_score(&seg.reading, b);
                    score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
                });
                let mut candidates: Vec<String> =
                    entries.iter().map(|e| e.surface.clone()).collect();
                // Always include the raw reading as a fallback
                if candidates.is_empty() || !candidates.contains(&seg.reading) {
                    candidates.push(seg.reading.clone());
                }
                SegmentState {
                    reading: seg.reading.clone(),
                    start: seg.start,
                    candidates,
                    selected: 0,
                    user_selected: false,
                }
            })
            .collect()
    }

    /// Compute effective score combining dictionary frequency and user learning.
    /// User learning is additive: even one selection gives a significant boost.
    fn effective_score(&self, reading: &str, entry: &DictionaryEntry) -> f64 {
        let freq_norm = (entry.frequency as f64) / 10000.0;
        let user = self.user_scorer.score(reading, &entry.surface);
        freq_norm + user * 2.0
    }

    /// Re-lookup candidates for a segment after its reading changed.
    fn relookup_segment(&mut self, idx: usize) {
        let reading = match self.conversion.as_ref() {
            Some(state) => state.segments[idx].reading.clone(),
            None => return,
        };
        let mut entries = self.dictionary.lookup(&reading);
        entries.sort_by(|a, b| {
            let score_a = self.effective_score(&reading, a);
            let score_b = self.effective_score(&reading, b);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut candidates: Vec<String> = entries.iter().map(|e| e.surface.clone()).collect();
        if candidates.is_empty() || !candidates.contains(&reading) {
            candidates.push(reading);
        }
        if let Some(state) = self.conversion.as_mut() {
            state.segments[idx].candidates = candidates;
            state.segments[idx].selected = 0;
        }
    }

    /// Build candidate sentences from segmented words.
    /// For each segment, pick the top candidates and combine.
    fn build_candidates(&self, segments: &[Segment]) -> Vec<String> {
        // Start with the top candidate for each segment (best conversion)
        let mut results = Vec::new();

        // Best candidate: top surface for each segment
        let best: String = segments
            .iter()
            .map(|seg| {
                seg.candidates
                    .first()
                    .map(|c| c.surface.as_str())
                    .unwrap_or(&seg.reading)
            })
            .collect();
        results.push(best);

        // Generate alternatives by swapping one segment at a time
        for (i, seg) in segments.iter().enumerate() {
            for candidate in seg.candidates.iter().skip(1).take(3) {
                let alt: String = segments
                    .iter()
                    .enumerate()
                    .map(|(j, s)| {
                        if j == i {
                            candidate.surface.as_str()
                        } else {
                            s.candidates
                                .first()
                                .map(|c| c.surface.as_str())
                                .unwrap_or(&s.reading)
                        }
                    })
                    .collect();
                if !results.contains(&alt) {
                    results.push(alt);
                }
            }
        }

        // Also include the raw kana as a candidate
        let raw_kana: String = segments.iter().map(|s| s.reading.as_str()).collect();
        if !results.contains(&raw_kana) {
            results.push(raw_kana);
        }

        results
    }

    /// Create grammar tokens from a candidate text and its segments.
    fn tokens_for_grammar(&self, _text: &str, segments: &[Segment]) -> Vec<GrammarToken> {
        segments
            .iter()
            .map(|seg| {
                let pos = seg
                    .candidates
                    .first()
                    .map(|c| c.pos)
                    .unwrap_or(crate::core::dictionary::PartOfSpeech::Other);
                GrammarToken {
                    surface: seg
                        .candidates
                        .first()
                        .map(|c| c.surface.clone())
                        .unwrap_or_else(|| seg.reading.clone()),
                    pos,
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub enum EngineAction {
    /// Key was buffered (incomplete romaji). Contains current preedit.
    Buffering(String),
    /// Preedit text was updated (kana produced). Contains current preedit.
    UpdatePreedit(String),
    /// Candidates are ready to display.
    ShowCandidates(Vec<ConversionCandidate>),
    /// Text was committed.
    Commit(String),
}

#[derive(Debug, Clone)]
pub struct ConversionCandidate {
    /// Converted text (kanji/mixed)
    pub text: String,
    /// Grammar score (0.0–1.0)
    pub grammar_score: f64,
    /// LLM score (0.0–1.0)
    pub llm_score: f64,
    /// Combined score (grammar * 0.4 + LLM * 0.6)
    pub score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_key_buffering() {
        let mut engine = ConversionEngine::new();
        match engine.process_key('k') {
            EngineAction::Buffering(preedit) => assert_eq!(preedit, "k"),
            _ => panic!("expected Buffering"),
        }
    }

    #[test]
    fn process_key_produces_kana() {
        let mut engine = ConversionEngine::new();
        engine.process_key('k');
        match engine.process_key('a') {
            EngineAction::UpdatePreedit(preedit) => assert_eq!(preedit, "か"),
            _ => panic!("expected UpdatePreedit"),
        }
    }

    #[test]
    fn preedit_shows_buffer() {
        let mut engine = ConversionEngine::new();
        engine.process_key('k');
        assert_eq!(engine.preedit(), "k");
        engine.process_key('a');
        assert_eq!(engine.preedit(), "か");
        engine.process_key('n');
        assert_eq!(engine.preedit(), "かn");
    }

    #[test]
    fn convert_basic() {
        let mut engine = ConversionEngine::new();
        // Type "kyou" → きょう
        for ch in "kyou".chars() {
            engine.process_key(ch);
        }
        let candidates = engine.convert();
        assert!(!candidates.is_empty());
        // Top candidate should be 今日 (highest frequency + kanji bonus)
        assert_eq!(candidates[0].text, "今日");
    }

    #[test]
    fn convert_sentence() {
        let mut engine = ConversionEngine::new();
        // Type "kyouhaiitenki" → きょうはいいてんき
        for ch in "kyouhaiitenki".chars() {
            engine.process_key(ch);
        }
        let candidates = engine.convert();
        assert!(!candidates.is_empty());
        // Some candidate should contain kanji conversion
        let any_has_kanji = candidates.iter().any(|c| {
            c.text.chars().any(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch))
        });
        assert!(any_has_kanji, "Expected kanji in candidates: {:?}", candidates.iter().map(|c| &c.text).collect::<Vec<_>>());
    }

    #[test]
    fn convert_empty() {
        let mut engine = ConversionEngine::new();
        let candidates = engine.convert();
        assert!(candidates.is_empty());
    }

    #[test]
    fn commit_resets_state() {
        let mut engine = ConversionEngine::new();
        for ch in "kyou".chars() {
            engine.process_key(ch);
        }
        let candidates = engine.convert();
        let committed = engine.commit(&candidates[0].text);
        assert_eq!(committed, candidates[0].text);
        assert_eq!(engine.preedit(), "");
    }

    #[test]
    fn candidates_have_scores() {
        let mut engine = ConversionEngine::new();
        for ch in "kyou".chars() {
            engine.process_key(ch);
        }
        let candidates = engine.convert();
        for c in &candidates {
            assert!(c.score >= 0.0);
            assert!(c.score <= 1.0);
            assert!(c.grammar_score >= 0.0);
            assert!(c.llm_score >= 0.0);
        }
    }

    #[test]
    fn kanji_ranked_above_kana() {
        let mut engine = ConversionEngine::new();
        for ch in "kyou".chars() {
            engine.process_key(ch);
        }
        let candidates = engine.convert();
        // Raw kana きょう should be ranked below kanji candidates
        let kana_pos = candidates.iter().position(|c| c.text == "きょう");
        let kanji_pos = candidates.iter().position(|c| c.text == "今日");
        if let (Some(kp), Some(kap)) = (kana_pos, kanji_pos) {
            assert!(kap < kp, "kanji should rank above kana");
        }
    }

    #[test]
    fn segment_conversion_basic() {
        let mut engine = ConversionEngine::new();
        for ch in "kyouhaiitenki".chars() {
            engine.process_key(ch);
        }
        let state = engine.start_conversion().unwrap();
        assert!(state.segments.len() >= 3);
        let text = state.composed_text();
        assert!(!text.is_empty());
    }

    #[test]
    fn segment_move_focus() {
        let mut engine = ConversionEngine::new();
        for ch in "kyouhaiitenki".chars() {
            engine.process_key(ch);
        }
        engine.start_conversion();
        let state = engine.move_focus(1).unwrap();
        assert_eq!(state.focus, 1);
        let state = engine.move_focus(-1).unwrap();
        assert_eq!(state.focus, 0);
    }

    #[test]
    fn segment_cycle_candidate() {
        let mut engine = ConversionEngine::new();
        for ch in "kyou".chars() {
            engine.process_key(ch);
        }
        engine.start_conversion();
        let state = engine.conversion_state().unwrap();
        let first = state.segments[0].candidates[0].clone();
        let state = engine.cycle_candidate(1).unwrap();
        let second = state.segments[0].candidates[state.segments[0].selected].clone();
        assert_ne!(first, second);
    }

    #[test]
    fn segment_resize() {
        let mut engine = ConversionEngine::new();
        for ch in "kyouhaiitenki".chars() {
            engine.process_key(ch);
        }
        engine.start_conversion();
        let orig_reading = engine.conversion_state().unwrap().segments[0].reading.clone();
        engine.resize_segment(1); // extend first segment by one char
        let new_reading = engine.conversion_state().unwrap().segments[0].reading.clone();
        assert_eq!(new_reading.chars().count(), orig_reading.chars().count() + 1);
    }

    #[test]
    fn segment_composed_text_and_ranges() {
        let mut engine = ConversionEngine::new();
        for ch in "kyou".chars() {
            engine.process_key(ch);
        }
        engine.start_conversion();
        let state = engine.conversion_state().unwrap();
        let ranges = state.segment_char_ranges();
        assert_eq!(ranges[0].0, 0);
        assert!(ranges.last().unwrap().1 > 0);
    }
}
