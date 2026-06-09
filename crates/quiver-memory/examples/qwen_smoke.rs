//! 真跑一次千问 embedding,验证 QwenEmbedder 的 API 形状/维度/耗时。
//!
//!   cargo run -p quiver-memory --example qwen_smoke --features qwen
//!
//! 会真调 DashScope(OpenAI 兼容端点)、产生少量费用;key 运行时从
//! ~/.agents/resources.json 的 llm.qwen.personal 读,绝不打印/提交。

#[cfg(feature = "qwen")]
fn main() -> anyhow::Result<()> {
    use quiver_memory::{Embedder, QwenEmbedder};

    let emb = QwenEmbedder::new("personal", "text-embedding-v2", 1536)?;
    let texts = vec![
        "数据库用 SQLite,记忆库和操作库分开存".to_string(),
        "the cat sat on the mat".to_string(),
    ];
    let t0 = std::time::Instant::now();
    let vecs = emb.embed(&texts)?;
    let dt = t0.elapsed();

    println!("✓ 千问 embedding: {} 条, 用时 {:?}, model={}", vecs.len(), dt, emb.model());
    for (i, v) in vecs.iter().enumerate() {
        let head: Vec<f32> = v.iter().take(3).copied().collect();
        println!("  [{i}] dim={} 前3={:?}", v.len(), head);
    }
    let identical = vecs.len() == 2 && vecs[0] == vecs[1];
    println!("两条不同文本向量相同? {identical}(应为 false)");
    Ok(())
}

#[cfg(not(feature = "qwen"))]
fn main() {
    eprintln!("需 --features qwen: cargo run -p quiver-memory --example qwen_smoke --features qwen");
}
