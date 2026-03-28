/// Grammar validation engine (derived from Promps-Ent)
///
/// Scores conversion candidates by grammatical correctness.
/// Filters out grammatically invalid candidates and ranks
/// remaining ones by structural plausibility.

use super::dictionary::PartOfSpeech;

/// Grammar score for a conversion candidate
#[derive(Debug, Clone)]
pub struct GrammarScore {
    /// Overall score (0.0 = invalid, 1.0 = perfect)
    pub score: f64,
    /// Detected issues
    pub issues: Vec<GrammarIssue>,
}

#[derive(Debug, Clone)]
pub struct GrammarIssue {
    pub rule: GrammarRule,
    pub severity: Severity,
    pub position: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy)]
pub enum GrammarRule {
    /// Particle without preceding noun
    ParticleWithoutNoun,
    /// Consecutive particles
    ConsecutiveParticles,
    /// Consecutive nouns without particle
    ConsecutiveNounsWithoutParticle,
    /// Missing subject marker (が)
    MissingSubject,
    /// Missing object marker (を)
    MissingObject,
}

pub struct GrammarEngine {}

impl GrammarEngine {
    pub fn new() -> Self {
        Self {}
    }

    /// Score a conversion candidate for grammatical correctness
    pub fn score(&self, _tokens: &[(String, PartOfSpeech)]) -> GrammarScore {
        // TODO: Implement grammar rules adapted from Promps-Ent
        GrammarScore {
            score: 1.0,
            issues: Vec::new(),
        }
    }
}
