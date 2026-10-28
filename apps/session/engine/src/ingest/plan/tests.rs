use session::library::publish::RemoteFile;

use super::{Decision, Kept, Local, Media, Same, Why, decide, needs_hash, travels, whole_id};

fn local(size: u64, modified: i128) -> Local {
    Local {
        rel: "Media/Bass.wav".into(),
        size,
        modified,
    }
}

fn remote(size: u64, content: Option<&str>) -> RemoteFile {
    RemoteFile {
        path: "Media/Bass.wav".into(),
        size,
        content: content.map(str::to_owned),
    }
}

/// BLAKE3 of `bytes`, hex.
fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[test]
fn the_whole_file_address_is_the_one_the_store_gives() {
    // files_store::chunk::Manifest for one chunk: b"FTSM", 1u8, 1u32 LE,
    // then the chunk's hash and length (u64 LE); its id is the BLAKE3 of
    // that. Written out by hand here so a change on either side shows.
    let bytes = b"riff";
    let mut manifest = b"FTSM\x01\x01\x00\x00\x00".to_vec();
    manifest.extend_from_slice(blake3::hash(bytes).as_bytes());
    manifest.extend_from_slice(&4u64.to_le_bytes());
    assert_eq!(
        whole_id(&hash(bytes), 4).as_deref(),
        Some(blake3::hash(&manifest).to_hex().as_str())
    );
    assert_eq!(whole_id("not hex", 4), None);
}

#[test]
fn each_file_is_sent_or_skipped_for_the_reason_that_holds() {
    let bytes = b"0123456789";
    let h = hash(bytes);
    let whole = whole_id(&h, 10).unwrap();
    let kept = Kept {
        size: 10,
        modified: 5,
        blake3: Some(h.clone()),
        remote: Some("chunked-id".into()),
        pending: None,
    };
    let cases: [(
        &str,
        Local,
        Option<RemoteFile>,
        Option<&Kept>,
        Option<&str>,
        Decision,
    ); 7] = [
        (
            "not there",
            local(10, 5),
            None,
            None,
            None,
            Decision::Send(Why::New),
        ),
        (
            "another size there",
            local(10, 5),
            Some(remote(9, Some(&whole))),
            Some(&kept),
            None,
            Decision::Send(Why::Changed),
        ),
        (
            "sent from here, untouched since, the server's copy as it was",
            local(10, 5),
            Some(remote(10, Some("chunked-id"))),
            Some(&kept),
            None,
            Decision::Skip(Same::Unchanged),
        ),
        (
            "put there some other way (rsync, then adopted): the same bytes",
            local(10, 9),
            Some(remote(10, Some(&whole))),
            None,
            Some(&h),
            Decision::Skip(Same::Content),
        ),
        (
            "touched here but the same bytes the server was sent",
            local(10, 9),
            Some(remote(10, Some("chunked-id"))),
            Some(&kept),
            Some(&h),
            Decision::Skip(Same::Content),
        ),
        (
            "the same size, other bytes",
            local(10, 9),
            Some(remote(10, Some(&whole))),
            None,
            Some(&hash(b"9876543210")),
            Decision::Send(Why::Changed),
        ),
        (
            "changed on the server since it was sent",
            local(10, 5),
            Some(remote(10, Some("someone-else"))),
            Some(&kept),
            Some(&h),
            Decision::Send(Why::Changed),
        ),
    ];
    for (case, local, remote, kept, hash, want) in cases {
        assert_eq!(decide(&local, remote.as_ref(), kept, hash), want, "{case}");
    }
}

#[test]
fn only_a_same_sized_file_the_ledger_cannot_vouch_for_is_hashed() {
    let kept = Kept {
        size: 10,
        modified: 5,
        remote: Some("id".into()),
        ..Kept::default()
    };
    assert!(
        !needs_hash(&local(10, 5), None, None),
        "nothing there to compare"
    );
    assert!(
        !needs_hash(&local(10, 5), Some(&remote(3, None)), None),
        "the size says"
    );
    assert!(!needs_hash(
        &local(10, 5),
        Some(&remote(10, Some("id"))),
        Some(&kept)
    ));
    assert!(needs_hash(
        &local(10, 6),
        Some(&remote(10, Some("id"))),
        Some(&kept)
    ));
    assert!(needs_hash(
        &local(10, 5),
        Some(&remote(10, Some("id"))),
        None
    ));
}

#[test]
fn what_travels_is_the_song_not_its_bookkeeping() {
    let all = |rel| travels(rel, Media::Originals);
    let proxies = |rel| travels(rel, Media::ProxiesOnly);
    for rel in [
        "Drive To LA.session",
        "Song.RPP",
        "Song.lrc",
        "Media/Bass.wav",
        "Samples/Imported/BASS - LIVE.wav",
        "Media/Proxies/Bass.ogg",
        "Media/Proxies/Bass.ogg.idx",
        "Media/Peaks/Bass.wav.sessionpeaks",
    ] {
        assert!(all(rel), "{rel} travels");
    }
    for rel in [
        ".fts-root.json",
        ".fts-files/store/chunks/ab",
        "Drive To LA.session-wal",
        "Drive To LA.session-tshm",
        "Media/Proxies/Bass.ogg.partial",
        "Media/peaks/Bass.wav.reapeaks",
        "Song.RPP-bak",
        "Media/.DS_Store",
        "__MACOSX/Song.RPP",
    ] {
        assert!(!all(rel), "{rel} stays here");
    }
    assert!(!proxies("Media/Bass.wav"));
    assert!(!proxies("Samples/Imported/Kick.aif"));
    assert!(!proxies("Media/Reference.wav"));
    assert!(proxies("Media/Proxies/Bass.ogg"));
    assert!(proxies("Song.session"));
    assert!(
        proxies("Samples/Imported/loop.mp3"),
        "no proxy is made of an mp3"
    );
}
