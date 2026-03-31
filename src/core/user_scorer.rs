/// User learning scorer for JaIM.
///
/// Records which (reading, surface) pairs the user selects and boosts
/// those pairs in future candidate ranking. Data is persisted to
/// `~/.local/share/jaim/user_scores.json`.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

pub struct UserScorer {
    /// Map of "reading|surface" -> selection count
    counts: HashMap<String, u32>,
    /// Whether there are unsaved changes
    dirty: bool,
}

impl UserScorer {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
            dirty: false,
        }
    }

    /// Record a user selection for the given (reading, surface) pair.
    /// Called only for segments where the user explicitly chose a candidate,
    /// so no additional filtering is needed here.
    pub fn record(&mut self, reading: &str, surface: &str) {
        let key = Self::key(reading, surface);
        *self.counts.entry(key).or_insert(0) += 1;
        self.dirty = true;
    }

    /// Score a (reading, surface) pair based on user history.
    /// Returns 0.0 if never selected. Uses absolute logarithmic scaling
    /// so that even a single selection provides meaningful signal.
    /// Saturates toward 1.0 around 20+ selections.
    pub fn score(&self, reading: &str, surface: &str) -> f64 {
        let key = Self::key(reading, surface);
        let count = match self.counts.get(&key) {
            Some(&c) => c,
            None => return 0.0,
        };

        // ln(1 + count) / ln(1 + 20) ≈ saturates at ~1.0 around 20 uses
        // 1 use → 0.23, 2 → 0.36, 5 → 0.59, 10 → 0.79, 20 → 1.0
        ((count as f64).ln_1p() / (20.0_f64).ln_1p()).min(1.0)
    }

    /// Default path for user scores file.
    pub fn default_path() -> io::Result<PathBuf> {
        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                PathBuf::from(home).join(".local/share")
            })
            .join("jaim");
        Ok(data_dir.join("user_scores.json"))
    }

    /// Load user scores from a JSON file. Returns empty scorer if file does not exist.
    pub fn load(path: &Path) -> io::Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let data = std::fs::read_to_string(path)?;
        let counts: HashMap<String, u32> = serde_json::from_str(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Self {
            counts,
            dirty: false,
        })
    }

    /// Save user scores to a JSON file. Only writes if there are unsaved changes.
    /// Uses atomic write (temp file + rename) to prevent corruption.
    pub fn save(&mut self, path: &Path) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_string_pretty(&self.counts)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, path)?;
        self.dirty = false;
        Ok(())
    }

    fn key(reading: &str, surface: &str) -> String {
        format!("{}|{}", reading, surface)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrecorded_score_is_zero() {
        let scorer = UserScorer::new();
        assert_eq!(scorer.score("きょう", "今日"), 0.0);
    }

    #[test]
    fn record_and_score() {
        let mut scorer = UserScorer::new();
        scorer.record("きょう", "今日");
        assert!(scorer.score("きょう", "今日") > 0.0);
    }

    #[test]
    fn more_selections_higher_score() {
        let mut scorer = UserScorer::new();
        for _ in 0..10 {
            scorer.record("きょう", "今日");
        }
        scorer.record("きょう", "京");
        assert!(scorer.score("きょう", "今日") > scorer.score("きょう", "京"));
    }

    #[test]
    fn single_selection_gives_boost() {
        let mut scorer = UserScorer::new();
        scorer.record("へんかん", "変換");
        // Even one selection should give a meaningful score
        assert!(scorer.score("へんかん", "変換") > 0.2);
    }

    #[test]
    fn kana_only_recorded() {
        let mut scorer = UserScorer::new();
        scorer.record("きょう", "きょう"); // same reading as surface — now recorded
        assert!(scorer.score("きょう", "きょう") > 0.0);
    }

    #[test]
    fn save_and_load() {
        let dir = std::env::temp_dir().join("jaim_test_scores");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_scores.json");

        let mut scorer = UserScorer::new();
        scorer.record("きょう", "今日");
        scorer.record("きょう", "今日");
        scorer.save(&path).unwrap();

        let loaded = UserScorer::load(&path).unwrap();
        assert_eq!(loaded.score("きょう", "今日"), scorer.score("きょう", "今日"));
    }

    #[test]
    fn load_nonexistent() {
        let scorer = UserScorer::load(Path::new("/tmp/nonexistent_jaim_scores.json")).unwrap();
        assert_eq!(scorer.score("test", "test"), 0.0);
    }
}
