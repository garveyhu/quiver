//! 真跑千问图书管理员判断,验证 QwenJudge 的裁决。
//!
//!   cargo run -p quiver-memory --example librarian_smoke --features qwen
//!
//! 会真调 DashScope chat、产生少量费用;key 运行时从 ~/.agents/resources.json 读。

#[cfg(feature = "qwen")]
fn main() -> anyhow::Result<()> {
    use quiver_memory::{ContradictionJudge, QwenJudge};

    let judge = QwenJudge::new("personal", "qwen-plus")?;

    let cases = [
        ("数据库用 SQLite 存储", "数据库改用 PostgreSQL 存储", "应判 incoming_supersedes"),
        ("前端用 React", "后端用 Rust 写", "应判 independent"),
    ];
    for (existing, incoming, expect) in cases {
        let t0 = std::time::Instant::now();
        let v = judge.judge(existing, incoming)?;
        println!("[{:?}] {:?} (用时 {:?})  ← 「{existing}」 vs 「{incoming}」", v, expect, t0.elapsed());
    }
    Ok(())
}

#[cfg(not(feature = "qwen"))]
fn main() {
    eprintln!("需 --features qwen: cargo run -p quiver-memory --example librarian_smoke --features qwen");
}
