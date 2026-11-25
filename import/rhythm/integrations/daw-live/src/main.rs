use std::{collections::HashSet, error::Error, path::Path};

use daw::{
    rpc::{BatchBuilder, BatchResponseExt, Daw, ItemHandle, Project},
    service::{
        BatchOp, BatchOpOutput, Duration, FadeShape, ItemRef, ItemsOp, ItemsOpOutput,
        PositionInSeconds, ProjectArg, ProjectContext, SourceType, StretchMarker,
        StretchMarkersClient, StretchTakeRef, TakeRef,
        take::{TakesOp, TakesOpOutput},
    },
};
use rhythm_editing::{AudioSegment, HybridPlan, SegmentKind};
use serde::Deserialize;

const EDIT_LABEL: &str = "Rhythm: quantize kit";

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
struct LiveEdit {
    project_guid: String,
    item_guids: Vec<String>,
    plan: HybridPlan,
}

struct Member {
    item: ItemHandle,
    start: f64,
    offset: f64,
    end: f64,
}

struct Slice {
    segment: AudioSegment,
    /// Playback and source distances from the item's start to the stretch.
    tail_start: Option<(f64, f64)>,
}

fn slices(segments: &[AudioSegment]) -> Vec<Slice> {
    let mut slices: Vec<Slice> = Vec::new();
    for segment in segments {
        if matches!(segment.kind, SegmentKind::TailStretch { .. })
            && let Some(previous) = slices.last_mut()
            && matches!(previous.segment.kind, SegmentKind::Slip)
            && (previous.segment.timeline_end - segment.timeline_start).abs() < 1e-7
            && (previous.segment.source_end - segment.source_start).abs() < 1e-7
        {
            let item = &mut previous.segment;
            previous.tail_start = Some((
                segment.timeline_start - item.timeline_start,
                segment.source_start - item.source_start,
            ));
            item.timeline_end = segment.timeline_end;
            item.source_end = segment.source_end;
            item.fade_out_s = segment.fade_out_s;
            item.kind = segment.kind.clone();
        } else {
            slices.push(Slice {
                segment: segment.clone(),
                tail_start: None,
            });
        }
    }
    slices
}

struct Lane(vox::Caller);

impl vox::FromVoxLane for Lane {
    const SERVICE_NAME: &'static str = "rhythm-live";

    fn from_vox_lane(caller: vox::Caller, _: Option<vox::ConnectionHandle>) -> Self {
        Self(caller)
    }
}

async fn connect(socket: &Path) -> Result<Daw> {
    let stream = tokio::net::UnixStream::connect(socket).await?;
    let connection = vox::initiator_on(vox_stream::StreamLink::unix(stream))
        .establish_connection()
        .await
        .map_err(|e| format!("DAW handshake: {e:?}"))?;
    let lane = connection
        .open_lane::<Lane>()
        .await
        .map_err(|e| format!("DAW lane: {e:?}"))?;
    Ok(Daw::new(lane.0))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [socket, request @ ..] = args.as_slice() else {
        return Err("usage: rhythm-daw-live SOCKET [EDIT.json | --undo PROJECT_GUID]".into());
    };
    let daw = connect(Path::new(socket)).await?;
    let project = daw.current_project().await?;
    match request {
        [] => {
            println!("project {}", project.guid());
            for item in project.items().all().await? {
                println!(
                    "{}  {}  {:.6}  {:.6}",
                    item.guid,
                    item.track_guid,
                    item.position.as_seconds(),
                    item.length.as_seconds()
                );
            }
        }
        [flag, guid] if flag == "--undo" => {
            if Some(project.guid()) != guid.to_str()
                || project.last_undo_label().await?.as_deref() != Some(EDIT_LABEL)
                || !project.undo().await?
            {
                return Err("no matching rhythm edit to undo in this project".into());
            }
        }
        [path] => {
            let edit = serde_json::from_slice(&std::fs::read(path)?)?;
            let count = apply(&daw, &project, &edit).await?;
            println!("Verified {count} live DAW items in {}", project.guid());
        }
        _ => return Err("usage: rhythm-daw-live SOCKET [EDIT.json | --undo PROJECT_GUID]".into()),
    }
    Ok(())
}

async fn preflight(project: &Project, edit: &LiveEdit) -> Result<Vec<Member>> {
    if project.guid() != edit.project_guid
        || edit.item_guids.is_empty()
        || edit.plan.segments.is_empty()
    {
        return Err("expected the named project, at least one item, and an edit plan".into());
    }
    let mut members: Vec<Member> = Vec::new();
    let mut tracks = HashSet::new();
    for id in &edit.item_guids {
        let item = project.items().by_guid(id).await?.ok_or("item missing")?;
        let info = item.info().await?;
        let take = item.active_take();
        let source = take.info().await?;
        let start = info.position.as_seconds();
        let end = start + info.length.as_seconds();
        let offset = source.start_offset.as_seconds();
        if !tracks.insert(info.track_guid)
            || info.locked
            || source.source_type != SourceType::Audio
            || (source.play_rate - 1.0).abs() > 1e-9
            || !take.stretch_markers().await?.is_empty()
            || members.first().is_some_and(|member| {
                (member.start - start).abs() > 1e-7 || (member.end - end).abs() > 1e-7
            })
        {
            return Err("select one unlocked, aligned, unstretched audio item per mic".into());
        }
        members.push(Member {
            item,
            start,
            offset,
            end,
        });
    }
    let first = &members[0];
    let mut end = first.start;
    for segment in &edit.plan.segments {
        if ![
            segment.timeline_start,
            segment.timeline_end,
            segment.source_start,
            segment.source_end,
            segment.fade_in_s,
            segment.fade_out_s,
        ]
        .iter()
        .all(|n| n.is_finite())
            || (segment.timeline_start - end).abs() > 1e-7
            || segment.timeline_end <= segment.timeline_start
            || segment.source_start < first.start - 1e-7
            || segment.source_end > first.end + 1e-7
            || segment.source_end <= segment.source_start
            || segment.fade_in_s < 0.0
            || segment.fade_out_s < 0.0
        {
            return Err(
                "plan needs ordered, contiguous segments backed by the aligned source items".into(),
            );
        }
        end = segment.timeline_end;
    }
    if (end - first.end).abs() > 1e-7 {
        return Err("the live plan must cover the full selected item span".into());
    }
    Ok(members)
}

async fn apply(daw: &Daw, project: &Project, edit: &LiveEdit) -> Result<usize> {
    let members = preflight(project, edit).await?;
    let slices = slices(&edit.plan.segments);
    let next_group = project
        .items()
        .all()
        .await?
        .iter()
        .filter_map(|item| item.group_id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("DAW group IDs exhausted")?;
    if u64::from(next_group) + slices.len() as u64 > i32::MAX as u64 {
        return Err("DAW group IDs exhausted".into());
    }
    let context = ProjectContext::project(project.guid());
    project.begin_undo_block(EDIT_LABEL).await?;
    let result = write_and_verify(daw, project, context, next_group, &members, &slices).await;
    project.end_undo_block(EDIT_LABEL).await?;
    if result.is_err() && project.last_undo_label().await?.as_deref() == Some(EDIT_LABEL) {
        if !project.undo().await? {
            return Err("live write failed and DAW could not roll it back".into());
        }
    }
    result
}

async fn write_and_verify(
    daw: &Daw,
    project: &Project,
    context: ProjectContext,
    first_group: u32,
    members: &[Member],
    slices: &[Slice],
) -> Result<usize> {
    let mut copies = BatchBuilder::new().with_fail_fast();
    let mut copy_steps = Vec::new();
    let started = std::time::Instant::now();
    for member in members {
        for _ in 1..slices.len() {
            copy_steps.push(copies.push_raw(
                BatchOp::Item(ItemsOp::DuplicateItem {
                    project: ProjectArg::Literal(context.clone()),
                    item: ItemRef::Guid(member.item.guid().to_owned()),
                }),
                |output, step| match output {
                    BatchOpOutput::Item(ItemsOpOutput::DuplicateItem(Some(guid))) => {
                        Ok(guid.clone())
                    }
                    _ => Err(daw::rpc::BatchExtractError::TypeMismatch {
                        step,
                        expected: "duplicate item GUID",
                    }),
                },
            ));
        }
    }
    let response = daw.execute_batch(copies.build()).await?;
    let ids = copy_steps
        .iter()
        .map(|step| response.get(step))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut ids = ids.into_iter();
    let mut places = BatchBuilder::new().with_fail_fast();
    let mut place_steps = Vec::new();
    let mut written = Vec::new();
    for member in members {
        let mut item_ids = vec![member.item.guid().to_owned()];
        item_ids.extend(ids.by_ref().take(slices.len() - 1));
        for (index, (guid, slice)) in item_ids.into_iter().zip(slices).enumerate() {
            let group = first_group + index as u32;
            let s = &slice.segment;
            let item = ItemRef::Guid(guid.clone());
            let project = ProjectArg::Literal(context.clone());
            let length = s.timeline_end - s.timeline_start;
            let offset = member.offset + s.source_start - member.start;
            for op in [
                ItemsOp::SetPosition {
                    project: project.clone(),
                    item: item.clone(),
                    position: PositionInSeconds::from_seconds(s.timeline_start),
                },
                ItemsOp::SetLength {
                    project: project.clone(),
                    item: item.clone(),
                    length: Duration::from_seconds(length),
                },
                ItemsOp::SetFadeIn {
                    project: project.clone(),
                    item: item.clone(),
                    length: Duration::from_seconds(s.fade_in_s.min(length)),
                    shape: FadeShape::Linear,
                },
                ItemsOp::SetFadeOut {
                    project: project.clone(),
                    item: item.clone(),
                    length: Duration::from_seconds(s.fade_out_s.min(length)),
                    shape: FadeShape::Linear,
                },
                ItemsOp::SetGroupId {
                    project: project.clone(),
                    item: item.clone(),
                    group_id: Some(group),
                },
            ] {
                place_steps.push(places.push_raw(BatchOp::Item(op), check_placement));
            }
            place_steps.push(places.push_raw(
                BatchOp::Take(TakesOp::SetStartOffset {
                    project,
                    item,
                    take: TakeRef::Active,
                    offset: Duration::from_seconds(offset),
                }),
                check_placement,
            ));
            written.push((member, guid, slice, group));
        }
    }
    let response = daw.execute_batch(places.build()).await?;
    for step in place_steps {
        response.get(&step)?;
    }
    let stretch = StretchMarkersClient::new(daw.caller().clone());
    let mut verify = Vec::with_capacity(written.len());
    for (member, guid, slice, group) in written {
        let markers = slice_markers(member, slice);
        if !markers.is_empty() {
            let location = StretchTakeRef::new(
                context.clone(),
                ItemRef::Guid(guid.clone()),
                TakeRef::Active,
            );
            stretch
                .set_stretch_markers(location, markers.clone())
                .await??;
        }
        verify.push((member, guid, slice, group, markers));
    }
    println!(
        "Applied live edits in {:.3}s before readback",
        started.elapsed().as_secs_f64()
    );
    for (member, guid, slice, group, markers) in &verify {
        let item = project
            .items()
            .by_guid(guid)
            .await?
            .ok_or("edited item missing")?;
        verify_slice(member, &item, slice, *group, markers).await?;
    }
    Ok(verify.len())
}

fn check_placement(
    output: &BatchOpOutput,
    step: u32,
) -> std::result::Result<(), daw::rpc::BatchExtractError> {
    let result = match output {
        BatchOpOutput::Item(
            ItemsOpOutput::SetPosition(r)
            | ItemsOpOutput::SetLength(r)
            | ItemsOpOutput::SetFadeIn(r)
            | ItemsOpOutput::SetFadeOut(r)
            | ItemsOpOutput::SetGroupId(r),
        )
        | BatchOpOutput::Take(TakesOpOutput::SetStartOffset(r)) => r,
        _ => {
            return Err(daw::rpc::BatchExtractError::TypeMismatch {
                step,
                expected: "item placement",
            });
        }
    };
    result
        .as_ref()
        .copied()
        .map_err(|error| daw::rpc::BatchExtractError::StepFailed(error.to_string()))
}

fn slice_markers(member: &Member, slice: &Slice) -> Vec<StretchMarker> {
    let segment = &slice.segment;
    let length = segment.timeline_end - segment.timeline_start;
    let offset = member.offset + segment.source_start - member.start;
    let mut markers = if matches!(segment.kind, SegmentKind::Slip) {
        Vec::new()
    } else {
        vec![
            StretchMarker::new(0.0, offset),
            StretchMarker::new(length, member.offset + segment.source_end - member.start),
        ]
    };
    if let Some((position, source)) = slice.tail_start {
        markers.insert(1, StretchMarker::new(position, offset + source));
    }
    markers
}

async fn verify_slice(
    member: &Member,
    item: &ItemHandle,
    slice: &Slice,
    group: u32,
    markers: &[StretchMarker],
) -> Result<()> {
    let segment = &slice.segment;
    let length = segment.timeline_end - segment.timeline_start;
    let offset = member.offset + segment.source_start - member.start;
    let take = item.active_take();
    let placed = item.info().await?;
    let read_markers = take.stretch_markers().await?;
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-6;
    if placed.group_id != Some(group)
        || !close(placed.position.as_seconds(), segment.timeline_start)
        || !close(placed.length.as_seconds(), length)
        || !close(take.info().await?.start_offset.as_seconds(), offset)
        || read_markers.len() != markers.len()
        || !read_markers.iter().zip(markers).all(|(actual, expected)| {
            close(actual.position, expected.position)
                && close(actual.source_position, expected.source_position)
        })
    {
        return Err(format!("DAW readback disagreed for item {}", item.guid()).into());
    }
    Ok(())
}
