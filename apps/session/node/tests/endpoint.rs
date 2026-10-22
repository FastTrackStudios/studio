//! STA-362 acceptance: the node starts on a temp root and returns its
//! endpoint id.

use session_node::{Node, PERSONAL_SLUG};

#[tokio::test(flavor = "multi_thread")]
async fn the_node_starts_on_a_temp_root_and_returns_its_endpoint_id() -> eyre::Result<()> {
    let tmp = tempfile::tempdir()?;
    let node = Node::start(tmp.path(), "Session Device").await?;

    eyre::ensure!(
        node.org_slugs().iter().any(|s| s == PERSONAL_SLUG),
        "a fresh node provisions its personal org, got {:?}",
        node.org_slugs()
    );
    let id = node
        .personal_endpoint_id()
        .ok_or_else(|| eyre::eyre!("the personal org's endpoint is bound"))?;
    eyre::ensure!(!id.is_empty(), "the endpoint id is not empty");
    eyre::ensure!(
        node.endpoint_id(PERSONAL_SLUG).as_deref() == Some(id.as_str()),
        "the id is stable across reads"
    );
    println!("endpoint id {id}");
    Ok(())
}
