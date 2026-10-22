//! Size probe for ADR 0002: a tokio runtime that, with `task-node`,
//! also boots the embedded Task node and prints its endpoint id.
//!
//! Built `--release` once without and once with the feature; the size
//! difference is what the node adds to an app that already holds tokio.

fn main() -> eyre::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run())
}

#[cfg(feature = "task-node")]
async fn run() -> eyre::Result<()> {
    let root = std::env::args_os().nth(1).map_or_else(
        || std::env::temp_dir().join("session-node-probe"),
        Into::into,
    );
    let node = session_node::Node::start(&root, "Probe").await?;
    println!(
        "endpoint {}",
        node.personal_endpoint_id().unwrap_or_default()
    );
    Ok(())
}

#[cfg(not(feature = "task-node"))]
async fn run() -> eyre::Result<()> {
    tokio::task::yield_now().await;
    println!("embedded node: {}", session_node::embedded());
    Ok(())
}
