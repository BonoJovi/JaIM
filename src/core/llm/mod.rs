/// LLM-based contextual ranking and final validation
///
/// Uses a local quantized model (Qwen2.5-0.5B Q4_K_M) for:
/// 1. Disambiguating homophone candidates
/// 2. Final check on dictionary-converted sentences
/// 3. Context-aware reranking of candidates
///
/// Runs on a background thread with KV cache pre-warming
/// to minimize latency at conversion time.

pub struct LlmEngine {
    // TODO: llama.cpp integration via llama-cpp-rs
}

impl LlmEngine {
    pub fn new() -> Self {
        Self {}
    }

    /// Score a candidate sentence for contextual naturalness
    pub fn score_candidate(&self, _context: &str, _candidate: &str) -> f64 {
        // TODO: Implement LLM scoring
        0.0
    }

    /// Rerank candidates by contextual naturalness
    pub fn rerank(&self, _context: &str, _candidates: &[String]) -> Vec<(String, f64)> {
        // TODO: Implement LLM reranking
        Vec::new()
    }

    /// Pre-warm KV cache with confirmed context (called during typing)
    pub fn warm_cache(&self, _context: &str) {
        // TODO: Background KV cache update
    }
}
