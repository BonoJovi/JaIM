/// Real LLM scorer using llama.cpp via llama-cpp-2 crate.
///
/// Loads a quantized GGUF model (Qwen2.5-0.5B Q4_K_M) and scores
/// candidate sentences by computing their log-likelihood given context.
/// Lower perplexity (higher log-prob) = more natural text.

use std::path::Path;
use std::sync::Mutex;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use log::{debug, info, warn};

use super::LlmScorer;

/// LLM scorer backed by a real llama.cpp model.
pub struct LlamaScorer {
    model: LlamaModel,
    context: Mutex<LlamaContext<'static>>,
    // We keep the backend alive for the lifetime of the scorer
    _backend: LlamaBackend,
}

// SAFETY: LlamaModel and LlamaContext are thread-safe in practice when
// accessed through a Mutex. The llama.cpp library handles its own synchronization.
unsafe impl Send for LlamaScorer {}
unsafe impl Sync for LlamaScorer {}

impl LlamaScorer {
    /// Load a GGUF model from the given path.
    /// Returns None if the model cannot be loaded.
    pub fn new(model_path: &Path) -> Option<Self> {
        if !model_path.exists() {
            warn!("LlamaScorer: model file not found: {}", model_path.display());
            return None;
        }

        let backend = LlamaBackend::init().ok()?;

        let model_params = LlamaModelParams::default();
        let model = match LlamaModel::load_from_file(&backend, model_path, &model_params) {
            Ok(m) => {
                info!("LlamaScorer: loaded model from {}", model_path.display());
                m
            }
            Err(e) => {
                warn!("LlamaScorer: failed to load model: {:?}", e);
                return None;
            }
        };

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(512));

        let context = match model.new_context(&backend, ctx_params) {
            Ok(ctx) => ctx,
            Err(e) => {
                warn!("LlamaScorer: failed to create context: {:?}", e);
                return None;
            }
        };

        // SAFETY: We store model and context together, and the model outlives the context
        // because both are in the same struct. We use transmute to erase the lifetime.
        let context: LlamaContext<'static> = unsafe { std::mem::transmute(context) };

        Some(Self {
            model,
            context: Mutex::new(context),
            _backend: backend,
        })
    }

    /// Load from the default model path (~/.local/share/jaim/models/).
    pub fn from_default_path() -> Option<Self> {
        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                std::path::PathBuf::from(home).join(".local/share")
            });
        let model_path = data_dir.join("jaim/models/qwen2.5-0.5b-instruct-q4_k_m.gguf");
        Self::new(&model_path)
    }

    /// Compute average log-probability of candidate tokens given context.
    /// Returns a value in roughly [0.0, 1.0] where higher = more natural.
    fn compute_score(&self, context: &str, candidate: &str) -> f64 {
        let full_text = format!("{}{}", context, candidate);

        let context_tokens = match self.model.str_to_token(context, AddBos::Always) {
            Ok(t) => t,
            Err(_) => return 0.5,
        };
        let full_tokens = match self.model.str_to_token(&full_text, AddBos::Always) {
            Ok(t) => t,
            Err(_) => return 0.5,
        };

        if full_tokens.len() <= context_tokens.len() {
            return 0.5; // candidate produced no new tokens
        }

        let candidate_start = context_tokens.len();
        let n_tokens = full_tokens.len();

        let mut ctx = match self.context.lock() {
            Ok(c) => c,
            Err(_) => return 0.5,
        };

        // Clear KV cache from previous scoring calls to avoid position conflicts.
        ctx.clear_kv_cache();

        // Build batch: all tokens, request logits for positions where we can
        // evaluate candidate tokens (i.e., positions candidate_start-1 .. n_tokens-2,
        // because logits at position i predict token i+1)
        let mut batch = LlamaBatch::new(n_tokens, 1);
        for (i, &token) in full_tokens.iter().enumerate() {
            // We need logits at position i if token i+1 is a candidate token
            let need_logits = i >= candidate_start.saturating_sub(1) && i < n_tokens - 1;
            if let Err(_) = batch.add(token, i as i32, &[0], need_logits) {
                return 0.5;
            }
        }

        if let Err(e) = ctx.decode(&mut batch) {
            debug!("LlamaScorer: decode error: {:?}", e);
            return 0.5;
        }

        // Compute average log-probability of candidate tokens
        let vocab_size = self.model.n_vocab();
        let mut total_log_prob = 0.0f64;
        let mut count = 0;

        // For each candidate token, get the logits at the previous position
        // and compute log P(token | context_so_far)
        let mut logit_idx = 0i32;
        for i in candidate_start..n_tokens {
            let logits = ctx.get_logits_ith(logit_idx);
            logit_idx += 1;

            let target_token = full_tokens[i];

            // log-softmax: log P(target) = logit[target] - log(sum(exp(logit)))
            let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let log_sum_exp: f32 = logits.iter()
                .map(|&l| (l - max_logit).exp())
                .sum::<f32>()
                .ln()
                + max_logit;

            let target_id = target_token.0 as usize;
            if target_id < vocab_size as usize {
                let log_prob = (logits[target_id] - log_sum_exp) as f64;
                total_log_prob += log_prob;
                count += 1;
            }
        }

        if count == 0 {
            return 0.5;
        }

        let avg_log_prob = total_log_prob / count as f64;

        // Convert log-probability to 0.0-1.0 range
        // avg_log_prob is typically in [-10, 0], where 0 = perfect prediction
        // Use sigmoid-like mapping: score = 1 / (1 + exp(-k * (avg_log_prob + offset)))
        let score = 1.0 / (1.0 + (-1.5 * (avg_log_prob + 3.0)).exp());

        score.clamp(0.0, 1.0)
    }
}

impl LlmScorer for LlamaScorer {
    fn score(&self, context: &str, candidate: &str) -> f64 {
        // Catch any panics from llama.cpp to prevent crashing the IBus process
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.compute_score(context, candidate)
        })) {
            Ok(score) => score,
            Err(_) => {
                warn!("LlamaScorer: compute_score panicked, returning fallback");
                0.5
            }
        }
    }

    fn warm_cache(&self, context: &str) {
        if context.is_empty() {
            return;
        }
        let tokens = match self.model.str_to_token(context, AddBos::Always) {
            Ok(t) => t,
            Err(_) => return,
        };
        if tokens.is_empty() {
            return;
        }

        let mut ctx = match self.context.lock() {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut batch = LlamaBatch::new(tokens.len(), 1);
        for (i, &token) in tokens.iter().enumerate() {
            let _ = batch.add(token, i as i32, &[0], false);
        }
        let _ = ctx.decode(&mut batch);
    }
}
