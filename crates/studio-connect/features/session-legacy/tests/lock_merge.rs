//! The per-device lock (`Hold` + `Bridge::merge_held`): while a device is
//! locked, a collaborator's edit is held in the doc and not made in its
//! engine, and its own edit stays in its engine and out of the doc;
//! unlocking merges both ways, and neither side's edit is lost. A device
//! that joins while locked keeps its engine until it unlocks.

#![cfg(feature = "engine")]

use std::sync::Arc;

use daw_proto::ProjectInfo;
use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_standalone::sync::Standalone;
use session::sync::engine::{MediaRoot, MemoryBlobs};
use session::sync::{Bridge, Hold, SessionDoc};

#[test]
fn locked_holds_both_ways_and_unlocking_merges_them() -> eyre::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()?;
    rt.block_on(inner())
}

async fn inner() -> eyre::Result<()> {
    let standalone = Standalone::new();
    for guid in ["mine", "theirs", "late"] {
        standalone.seed_project(ProjectInfo {
            guid: guid.into(),
            name: guid.into(),
            path: String::new(),
        });
    }
    let bundle = daw_standalone::bootstrap::build_in_process_daw(standalone.clone()).await?;
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(bundle.daw.clone(), runtime);
    let direct: Option<Arc<dyn session::sync::engine::Direct>> = Some(Arc::new(standalone.clone()));
    let media = MediaRoot(std::path::PathBuf::from("/"));
    let blobs = Arc::new(MemoryBlobs::default());

    // This device hosts the song; a collaborator (an agent, a member) is in it.
    let mine = bundle.daw.project("mine").await?;
    let kick = mine.tracks().add_with_guid("{KICK}", "Kick", None).await?;
    let mut me = Bridge::host(
        mine.clone(),
        media.clone(),
        SessionDoc::new(),
        String::new(),
        blobs.clone(),
        direct.clone(),
    )
    .await?;
    let theirs = bundle.daw.project("theirs").await?;
    let mut them = Bridge::join(
        theirs.clone(),
        media.clone(),
        me.doc().fork(),
        blobs.clone(),
        direct.clone(),
    )
    .await?;
    let volume = || async { kick.info().await.map(|t| t.volume) };
    let before = volume().await?;
    assert_ne!(before, 0.5, "the kick starts elsewhere");

    // Locked.
    let mut hold = Hold::new(false);
    assert!(hold.set_locked(true).is_empty());

    // They turn the kick down; it arrives here and is held.
    theirs
        .tracks()
        .by_guid("{KICK}")
        .await?
        .ok_or_else(|| eyre::eyre!("their kick"))?
        .set_volume(0.5)
        .await?;
    assert!(them.local_step("Kick down").await?);
    me.doc()
        .import(&them.doc().updates_since(&me.doc().version())?, "remote")?;
    assert!(!hold.remote(0), "held, not applied");
    assert_eq!(volume().await?, before, "their edit waits in the doc");

    // A hit added here stays here.
    mine.items()
        .add_with_guid(
            "{KICK}",
            "{HIT}",
            PositionInSeconds::from_seconds(1.0),
            Duration::from_seconds(0.5),
        )
        .await?;
    assert!(!hold.local(0), "kept, not recorded");
    assert_eq!(
        me.doc().read().items.len(),
        0,
        "the doc has not heard of it"
    );
    assert_eq!(hold.held(), 1);

    // Unlocked: merged both ways.
    let held = hold.set_locked(false);
    assert_eq!(held.songs.iter().copied().collect::<Vec<_>>(), [0]);
    assert_eq!((held.local, held.remote), (1, 1));
    me.merge_held("Edits while locked").await?;
    assert_eq!(volume().await?, 0.5, "their edit is made here");
    assert_eq!(mine.items().all().await?.len(), 1, "the hit is still here");
    assert_eq!(me.doc().read().items.len(), 1, "and recorded");
    assert_eq!(me.doc().read().tracks.len(), 1);

    // …and it reaches them, their own edit standing.
    them.doc()
        .import(&me.doc().updates_since(&them.doc().version())?, "remote")?;
    them.remote_changed().await?;
    assert_eq!(theirs.items().all().await?.len(), 1, "the hit reached them");
    let their_volume = theirs
        .tracks()
        .by_guid("{KICK}")
        .await?
        .ok_or_else(|| eyre::eyre!("their kick"))?
        .info()
        .await?
        .volume;
    assert_eq!(their_volume, 0.5, "their edit stands");

    // A device that joins while locked keeps its engine until it unlocks.
    let late = bundle.daw.project("late").await?;
    let mut held_join = Bridge::join_held(
        late.clone(),
        media.clone(),
        me.doc().fork(),
        blobs.clone(),
        direct.clone(),
    )
    .await?;
    assert!(
        late.tracks().by_guid("{KICK}").await?.is_none(),
        "nothing made in a locked engine"
    );
    held_join.merge_held("Edits while locked").await?;
    assert!(
        late.tracks().by_guid("{KICK}").await?.is_some(),
        "unlocked, it matches the set"
    );
    assert_eq!(late.items().all().await?.len(), 1);
    Ok(())
}
