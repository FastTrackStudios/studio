//! Something over the network, tried again until it answers.

/// Try `attempt` until it works — or `limit` times — waiting between
/// tries from a second, doubling to fifteen; `failed` hears why each one
/// did not, and when the next is.
///
/// # Errors
///
/// The last try's, once `limit` tries have failed.
pub async fn until_ok<T, Fut>(
    limit: Option<u32>,
    mut attempt: impl FnMut() -> Fut,
    mut failed: impl FnMut(String),
) -> eyre::Result<T>
where
    Fut: std::future::Future<Output = eyre::Result<T>>,
{
    let mut wait = std::time::Duration::from_secs(1);
    let mut tries = 0u32;
    loop {
        match attempt().await {
            Ok(done) => return Ok(done),
            Err(e) => {
                tries += 1;
                if limit.is_some_and(|limit| tries >= limit) {
                    return Err(e);
                }
                tracing::debug!(error = %e, tries, "trying again");
                failed(format!("{e} — trying again in {} s", wait.as_secs()));
                architect::platform::sleep(wait).await;
                wait = (wait * 2).min(std::time::Duration::from_secs(15));
            }
        }
    }
}
