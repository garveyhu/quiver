//! 真跑千问经理大脑,验证 QwenBrain 按局面输出合法 Decision。
//!
//!   cargo run -p quiver-orchestrator --example manager_smoke --features qwen
//!
//! 会真调 DashScope chat、产生少量费用;key 运行时从 ~/.agents/resources.json 读。

#[cfg(feature = "qwen")]
fn main() -> anyhow::Result<()> {
    use quiver_orchestrator::{ManagerBrain, ManagerContext, QwenBrain};

    let brain = QwenBrain::new("personal", "qwen-plus")?;

    // 局面一:有预算、在途未满、有排队 → 期望经理 spawn。
    let busy = ManagerContext {
        inflight: 0,
        queued: 2,
        max_inflight: 3,
        budget_remaining_usd: 10.0,
        brief: "项目:一个 Rust CLI。待办:写 README、加 --version。".into(),
    };
    let t0 = std::time::Instant::now();
    let d1 = brain.decide(&busy)?;
    println!("[忙·有排队] → {:?}  (用时 {:?})", d1, t0.elapsed());

    // 局面二:没预算 → 期望经理别派活(noop/escalate)。
    let broke = ManagerContext {
        budget_remaining_usd: 0.0,
        ..busy.clone()
    };
    let d2 = brain.decide(&broke)?;
    println!("[没预算] → {:?}", d2);

    Ok(())
}

#[cfg(not(feature = "qwen"))]
fn main() {
    eprintln!("需 --features qwen: cargo run -p quiver-orchestrator --example manager_smoke --features qwen");
}
