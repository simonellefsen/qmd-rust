//! Embedding generation for Area 2.
//!
//! This module provides the trait + concrete backends for turning text chunks
//! into vectors that can be stored in `content_vectors` and used for
//! `vec:` / hybrid search in `query` and `vsearch`.

use anyhow::Result;

pub trait Embedder: Send + Sync {
    /// Returns the embedding dimension this model produces.
    fn dimension(&self) -> usize;

    /// Name / identifier of the model (for fingerprinting).
    fn model_id(&self) -> &str;

    /// Embed a batch of texts. All texts should be reasonably short chunks.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}

/// Placeholder / no-op embedder used until a real backend is configured.
pub struct NoopEmbedder;

impl Embedder for NoopEmbedder {
    fn dimension(&self) -> usize {
        0
    }
    fn model_id(&self) -> &str {
        "none"
    }
    fn embed_batch(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(vec![])
    }
}

#[cfg(feature = "llama-embed")]
pub mod llama;

/// Returns the default embedder based on available features and config.
pub fn default_embedder() -> Box<dyn Embedder> {
    #[cfg(feature = "llama-embed")]
    {
        // Real LlamaEmbedder (GGUF via llama-cpp-2 + Metal). Model path from
        // QMD_EMBED_MODEL or config; actual load is lazy on first embed_batch.
        Box::new(llama::LlamaEmbedder::new())
    }
    #[cfg(not(feature = "llama-embed"))]
    {
        Box::new(NoopEmbedder)
    }
}

/// Returns an embedder suitable for reranking (I2 real reranker).
/// Prefers QMD_RERANK_MODEL / models.rerank from config (via llama path).
/// Falls back to the regular embed model if no rerank spec. This lets
/// `models.rerank` drive the post-fusion rerank step in query when present,
/// while reusing the exact existing LlamaEmbedder load/embed code (smallest).
/// No-op when feature absent.
pub fn default_reranker() -> Box<dyn Embedder> {
    #[cfg(feature = "llama-embed")]
    {
        Box::new(llama::LlamaEmbedder::for_rerank())
    }
    #[cfg(not(feature = "llama-embed"))]
    {
        Box::new(NoopEmbedder)
    }
}
