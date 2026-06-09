//! Embedding producer for §6.7 vector recall.
//!
//! [`Embedder`] is the injectable seam — per the build plan, LLM/embedding calls go
//! through a trait so tests use a free, deterministic [`FakeEmbedder`] while real
//! runs use 千问. The real [`QwenEmbedder`] (behind the `qwen` cargo feature) reads
//! its key at runtime from `~/.agents/resources.json` — never hardcoded, never
//! logged, never committed. 千问凭证 + HTTP 走共享 [`quiver_llm`] 适配层。

/// Produces dense embeddings for fact text. One call may embed a batch (order-preserving).
pub trait Embedder {
    /// Embed each input string; returns one vector per input, in the same order.
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;
    /// Dimensionality this embedder produces.
    fn dim(&self) -> usize;
    /// Model id stamped onto facts (so a model swap knows what to recompute).
    fn model(&self) -> &str;
}

/// Deterministic, offline, free embedder for tests + `simulate` mode. NOT semantic —
/// same text always maps to the same vector and different text to a different one,
/// so retrieval logic can be exercised without a network call or API key.
pub struct FakeEmbedder {
    dim: usize,
}

impl FakeEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

impl Default for FakeEmbedder {
    fn default() -> Self {
        Self { dim: 8 }
    }
}

impl Embedder for FakeEmbedder {
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| fake_vec(t, self.dim)).collect())
    }
    fn dim(&self) -> usize {
        self.dim
    }
    fn model(&self) -> &str {
        "fake-deterministic"
    }
}

/// FNV-1a seed over the bytes, then a splitmix-style deterministic fill → values in [-1,1).
fn fake_vec(text: &str, dim: usize) -> Vec<f32> {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (0..dim)
        .map(|i| {
            let mut x = h.wrapping_add((i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
            x ^= x >> 33;
            x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
            x ^= x >> 33;
            (x % 2000) as f32 / 1000.0 - 1.0
        })
        .collect()
}

/// Real 千问 embedder via the shared [`quiver_llm`] adapter (`text-embedding-v2`,
/// 1536-dim). Behind the `qwen` feature so default builds stay offline. Creds read
/// at runtime from `~/.agents/resources.json`.
#[cfg(feature = "qwen")]
pub struct QwenEmbedder {
    creds: quiver_llm::QwenCreds,
    model: String,
    dim: usize,
}

#[cfg(feature = "qwen")]
impl QwenEmbedder {
    /// `profile` = `"personal"`/`"company"`; `model` e.g. `"text-embedding-v2"` (1536-dim).
    pub fn new(profile: &str, model: impl Into<String>, dim: usize) -> anyhow::Result<Self> {
        Ok(Self {
            creds: quiver_llm::load_qwen_creds(profile)?,
            model: model.into(),
            dim,
        })
    }
}

#[cfg(feature = "qwen")]
impl Embedder for QwenEmbedder {
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        quiver_llm::qwen_embed(&self.creds, &self.model, texts)
    }
    fn dim(&self) -> usize {
        self.dim
    }
    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_embedder_is_deterministic_and_distinct() {
        let e = FakeEmbedder::new(8);
        let a1 = e.embed(&["hello".into()]).unwrap();
        let a2 = e.embed(&["hello".into()]).unwrap();
        let b = e.embed(&["world".into()]).unwrap();
        assert_eq!(a1[0].len(), 8, "honors dim");
        assert_eq!(a1, a2, "same text → same vector");
        assert_ne!(a1[0], b[0], "different text → different vector");
        assert_eq!(e.dim(), 8);
    }
}
