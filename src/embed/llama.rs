//! llama-cpp-2 based embedder (behind `llama-embed` feature).
//!
//! Real implementation for Area 2 sub-slice:
//! - Loads a local GGUF embedding model (user provides via QMD_EMBED_MODEL env
//!   or `models.embed` in ~/.config/qmd/index.yml; no auto-download to keep
//!   the feature light and avoid extra deps like hf_hub in the default dist).
//! - Uses the embeddings context + batch decode path from the upstream
//!   examples/embeddings to produce *meaningful* (non-zero) vectors.
//! - Model id derived from config for stable fingerprinting.
//!
//! Load model: Backend is cached globally (OnceLock). Full LlamaModel +
//! LlamaContext creation still happens on every embed_batch (i.e. per file in
//! current cmd_embed callers). This is a known perf consideration for
//! realistic `update --embed` on larger wikis (see HIGH Issue 1 in review).
//! It is the smallest viable approach that delivered correct real embeddings
//! without lifetime/Send/Sync complexity in the first sub-slice. Caching the
//! model/ctx across batches on the same embedder instance is the documented
//! immediate next micro-step (and will be a small follow-up change).

use super::Embedder;
use crate::db::{expand_tilde, load_config};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct LlamaEmbedder {
    /// Resolved model path (may contain ~ which is expanded on use).
    model_path: String,
    /// Stable identifier for fingerprinting (the spec the user provided, or
    /// fallback). Changing this invalidates prior vectors (intended).
    model_id: String,
    /// Real dimension discovered from the first successful model output in this
    /// embedder instance (populated on first real embed_batch). Ensures all
    /// vectors (including zero-pads for empty chunks) have consistent length
    /// within and across batches for the same model. Mitigates mixed-length
    /// and similarity-score corruption (HIGH Issue 2).
    real_dimension: std::sync::OnceLock<usize>,
}

impl Default for LlamaEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl LlamaEmbedder {
    /// Construct. Resolution of actual GGUF happens on first embed (or via
    /// QMD_EMBED_MODEL / config at construction time for the id).
    pub fn new() -> Self {
        let spec = std::env::var("QMD_EMBED_MODEL").ok().or_else(|| {
            load_config()
                .ok()
                .and_then(|cfg| cfg.models.and_then(|m| m.embed))
        });
        let model_id = spec
            .clone()
            .unwrap_or_else(|| "local-gguf-embed".to_string());
        let model_path =
            spec.unwrap_or_else(|| "~/models/embeddinggemma-300M-Q8_0.gguf".to_string());
        Self {
            model_path,
            model_id,
            real_dimension: std::sync::OnceLock::new(),
        }
    }

    /// Minimal stub so `default_reranker()` (and the feature-gated rerank path
    /// in query) compiles when the `llama-embed` feature is enabled.
    /// The real for_rerank (separate rerank model, cosine post-fusion, etc.)
    /// is part of the larger pending Iteration 2 work and will overlay this.
    pub fn for_rerank() -> Self {
        Self::new()
    }

    fn resolve_model_path(&self) -> Result<PathBuf> {
        let expanded = expand_tilde(&self.model_path);
        let pb = PathBuf::from(expanded);
        if !pb.exists() {
            anyhow::bail!(
                "GGUF embedding model not found at {:?}\n\
                 Set QMD_EMBED_MODEL=/absolute/path/to/xxx.gguf (recommended)\n\
                 or models.embed in ~/.config/qmd/index.yml.\n\
                 Example: embeddinggemma-300M-Q8_0.gguf (download from HF ggml-org).",
                pb
            );
        }
        Ok(pb)
    }
}

impl Embedder for LlamaEmbedder {
    fn dimension(&self) -> usize {
        // Return real dim once discovered from a successful embedding (populated in
        // embed_batch on first real output). Falls back to advisory 768. This ensures
        // consistent vector lengths (including for zero-pads) and prevents mixed-len
        // or cosine=0 corruption within a command (addresses HIGH Issue 2).
        self.real_dimension.get().copied().unwrap_or(768)
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let model_path = self.resolve_model_path()?;

        // --- Real llama-cpp-2 embedding path (ported/adapted from upstream examples/embeddings) ---
        use llama_cpp_2::context::params::LlamaContextParams;
        use llama_cpp_2::llama_backend::LlamaBackend;
        use llama_cpp_2::llama_batch::LlamaBatch;
        use llama_cpp_2::model::params::LlamaModelParams;
        use llama_cpp_2::model::{AddBos, LlamaModel};

        // Global backend (init is typically a one-time lib setup; OnceLock prevents
        // repeated init churn mentioned in HIGH Issue 1). Model load + ctx creation
        // remain per-embed_batch for this slice (see below).
        static LLAMA_BACKEND: std::sync::OnceLock<LlamaBackend> = std::sync::OnceLock::new();
        let backend = LLAMA_BACKEND.get_or_init(|| {
            LlamaBackend::init().expect("failed to initialize llama backend (Metal/CPU)")
        });

        // One-time notice (per process) about the current per-embed_batch model load.
        // This is the main perf consideration for `update --embed` on >~10 files in
        // this first real sub-slice. Full caching of LlamaModel + LlamaContext across
        // multiple embed_batch calls on the same LlamaEmbedder instance (or a
        // process-global model cache) is the immediate follow-up micro-step.
        static FIRST_LOAD_NOTICED: AtomicBool = AtomicBool::new(false);
        if !FIRST_LOAD_NOTICED.swap(true, Ordering::Relaxed) {
            eprintln!("(loading GGUF embedding model — first time in this process; subsequent files still pay per-batch load until caching added)");
        }

        // High n_gpu_layers works for Metal when the crate was built with the "metal" feature.
        // (No cfg guard here; the optional dep in Cargo.toml already selects Metal on macOS.)
        let model_params = LlamaModelParams::default().with_n_gpu_layers(1000);

        let model = LlamaModel::load_from_file(backend, model_path, &model_params)
            .context("failed to load GGUF model (ensure it is a valid *embedding* GGUF, not a text-gen only model)")?;

        let n_threads: i32 = std::thread::available_parallelism()
            .map(|p| p.get() as i32)
            .unwrap_or(4);

        let ctx_params = LlamaContextParams::default()
            .with_n_threads_batch(n_threads)
            .with_embeddings(true);

        let mut ctx = model
            .new_context(backend, ctx_params)
            .context("failed to create embedding context (does the model support embeddings?)")?;

        let n_ctx = ctx.n_ctx() as usize;
        let mut output = Vec::with_capacity(texts.len());

        for (i, text) in texts.iter().enumerate() {
            if text.trim().is_empty() {
                let dim = self
                    .real_dimension
                    .get()
                    .copied()
                    .unwrap_or(self.dimension());
                output.push(vec![0.0f32; dim]);
                continue;
            }

            let tokens = match model.str_to_token(text, AddBos::Always) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!(
                        "  embed chunk {} tokenize failed ({}): using zero vector",
                        i, e
                    );
                    let dim = self
                        .real_dimension
                        .get()
                        .copied()
                        .unwrap_or(self.dimension());
                    output.push(vec![0.0f32; dim]);
                    continue;
                }
            };

            let tok_len = tokens.len();
            if tok_len == 0 {
                let dim = self
                    .real_dimension
                    .get()
                    .copied()
                    .unwrap_or(self.dimension());
                output.push(vec![0.0f32; dim]);
                continue;
            }
            if tok_len > n_ctx {
                // crude truncate for this slice (real impl would split or error)
                // we just use what fits
            }

            let batch_size = (tok_len + 8).max(128);
            let mut batch = LlamaBatch::new(batch_size, 1);
            if let Err(e) = batch.add_sequence(&tokens[..tok_len.min(n_ctx)], 0, false) {
                eprintln!(
                    "  embed chunk {} batch add failed ({}): using zero vector",
                    i, e
                );
                let dim = self
                    .real_dimension
                    .get()
                    .copied()
                    .unwrap_or(self.dimension());
                output.push(vec![0.0f32; dim]);
                continue;
            }

            ctx.clear_kv_cache();
            if let Err(e) = ctx.decode(&mut batch) {
                eprintln!(
                    "  embed chunk {} decode failed ({}): using zero vector",
                    i, e
                );
                let dim = self
                    .real_dimension
                    .get()
                    .copied()
                    .unwrap_or(self.dimension());
                output.push(vec![0.0f32; dim]);
                batch.clear();
                continue;
            }

            let embedding = match ctx.embeddings_seq_ith(0) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!(
                        "  embed chunk {} embeddings_seq_ith failed ({}): using zero vector",
                        i, e
                    );
                    let dim = self
                        .real_dimension
                        .get()
                        .copied()
                        .unwrap_or(self.dimension());
                    output.push(vec![0.0f32; dim]);
                    batch.clear();
                    continue;
                }
            };

            // First real vector: record its length as the authoritative dim for this
            // embedder instance (and all future pads / dimension() calls). Ensures
            // consistent lengths even across multiple embed_batch calls (e.g. many
            // files in one `update --embed`).
            let _ = self.real_dimension.set(embedding.len());
            output.push(embedding.to_vec());
            batch.clear();
        }

        Ok(output)
    }
}
