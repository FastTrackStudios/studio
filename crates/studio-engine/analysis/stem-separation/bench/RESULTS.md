# Open-model pilot results

These runs used fixed excerpts and aligned WAV references. See the
[reproduction guide](PILOT.md) for fixture construction, model hashes, and
commands. The excerpts are small enough to debug the protocol, and each
task needs more songs before we rank models across a corpus.

## Four-stem song separation

On the same 20-second “A Reason To Leave” excerpt, the equal-weight
SCNet and BS-RoFormer average improved global SDR on all four stems
compared with either component. SCNet's single-model results were close
enough to warrant measuring its runtime and memory against the ensemble
on an otherwise idle host.

| Model | Vocals | Drums | Bass | Other |
| --- | ---: | ---: | ---: | ---: |
| HTDemucs | 6.62 | 6.51 | 10.70 | 9.05 |
| HTDemucs six-stem, grouped | 6.65 | 6.03 | 10.63 | 8.04 |
| BS-RoFormer | 8.20 | 7.21 | 9.74 | 10.10 |
| SCNet | 8.31 | 7.13 | 10.78 | 10.46 |
| SCNet + BS-RoFormer mean | 8.68 | 7.75 | 10.82 | 10.72 |

Values are full-waveform global SDR in dB. SCNet alone had a smaller
reconstruction residual than the mean ensemble. The two measurements
answer different questions about re-summing the mix and assigning audio
to the right instrument.

## Lead and backing vocals

On seconds 95–115 of the same song's voice bus, the original UVR MDX-Net
Karaoke checkpoint scored 4.13 dB on lead and 2.08 dB on backing. Karaoke 2
scored 2.52 dB and 0.66 dB. The lead group contains the lead double. A full
mix test still needs an upstream vocal separator.

## Crescendum drum cascade

On seconds 26–38 of “the plague,” a controlled mixture adds the non-drum
stems of the Cambridge excerpt at equal RMS to the known Crescendum
drum mix. HTDemucs recovered the drum bus at 10.68 dB global SDR.

| Audio for the same detector | Kick F1 | Snare F1 | Toms F1 |
| --- | ---: | ---: | ---: |
| Contaminated mix | 0.484 | 0.355 | 0.406 |
| HTDemucs drum bus | 0.538 | 0.412 | 0.407 |
| HTDemucs → LarsNet | 0.602 | 0.528 | 0.477 |
| HTDemucs → DrumSep six-stem | 0.679 | 0.657 | 0.511 |

These F1 values use ±20 ms matching to healthy piezo trigger labels. The
accompaniment is from another recording, so the result describes a
repeatable interference test. The original mixed Crescendum recording
requires a matched full-mix export.

## Crescendum kit checkpoint comparison

The five-stem DrumSep checkpoint stayed within 0.30 dB of six-stem
DrumSep on the median matched global SDR for every group across the two
excerpts. Trigger timing showed a larger difference on toms.

| Comped excerpt | LarsNet tom F1 | DrumSep six-stem | DrumSep five-stem |
| --- | ---: | ---: | ---: |
| the plague | 0.494 | 0.552 | 0.593 |
| it knows my name | 0.433 | 0.700 | 0.345 |

The six-stem checkpoint is the stronger default for the current
trigger-based workflow. The five-stem checkpoint deserves a larger trial
because its result depends heavily on the excerpt. Median BSS Eval SDR differences stayed within 0.10 dB for each
group. For the combined
cymbal group, the only waveform proxy in these runs is the ride mic.

## Matched six-stem song comparison

BS-RoFormer SW improved guitar on both excerpts. It also improved acoustic
piano on “Milk Cow Blues,” while both models struggled with electric piano
on “A Reason To Leave.” Values are global SDR in dB.

| Reference | HTDemucs six-stem | BS-RoFormer SW |
| --- | ---: | ---: |
| Guitar, A Reason To Leave | 4.64 | 7.77 |
| Electric piano, A Reason To Leave | −1.46 | −1.66 |
| Guitar, Milk Cow Blues | 2.18 | 8.40 |
| Acoustic piano, Milk Cow Blues | 1.02 | 6.63 |

The “A Reason To Leave” labels score electric piano as piano and keep
Rhodes and vibraphone in other. This matters when choosing a specialist
keys model and planning a shared evaluation taxonomy. Expand the trial
with independently labeled acoustic piano, electric keys, and dense
guitar fixtures before choosing a default. BS-RoFormer SW remains a local
evaluation candidate because the checkpoint's weight terms are unknown.

## Crescendum session with its own accompaniment

The trigger-free REAPER render of “the plague” aligned at zero sample lag
to the comped kit reference. A separate `Drums` solo render used the same
timeline, master attenuation, and local plugin configuration. Relative
to that solo render, the untreated mix was **3.30 dB global SDR** and the
HTDemucs drum estimate was **10.71 dB**.

| Audio for the same detector | Kick F1 | Snare F1 | Toms F1 |
| --- | ---: | ---: | ---: |
| Trigger-free session mix | 0.517 | 0.343 | 0.467 |
| HTDemucs drum estimate | 0.473 | 0.409 | 0.439 |
| HTDemucs → LarsNet | 0.507 | 0.434 | 0.556 |
| HTDemucs → DrumSep six-stem | 0.606 | 0.548 | 0.419 |

DrumSep led on kick and snare. LarsNet led on toms, reversing the
ranking from the controlled accompaniment test. The numbers are onset
F1 at ±20 ms. REAPER reported a project load warning on the initial
attempt. Media was checked and the rerender used the local plugin
configuration, so the result characterizes this benchmark render.

The second session, “it knows my name,” aligned within one sample and gave
the model more accompaniment to remove. The untreated mix scored
**−6.82 dB global SDR** against the drum solo. The HTDemucs drum estimate
scored **6.87 dB** against the same reference.

| Audio for the same detector | Kick F1 | Snare F1 | Toms F1 |
| --- | ---: | ---: | ---: |
| Trigger-free session mix | 0.286 | 0.292 | 0.312 |
| HTDemucs drum estimate | 0.215 | 0.381 | 0.413 |
| HTDemucs → LarsNet | 0.537 | 0.387 | 0.357 |
| HTDemucs → DrumSep six-stem | 0.718 | 0.778 | 0.667 |

DrumSep led kick and snare on both session mixes. The toms result favored
a different model on each song, so a fixed toms selection needs more
matched sessions.

## Electric-keys specialist

The digital-piano head improved keys SDR on three matched excerpts.
Each 20-second mix contains the named electric-keys target and separate
backing tracks. Values are global SDR in dB.

| Excerpt and target | HTDemucs piano | Digital-piano head | Banquet |
| --- | ---: | ---: | ---: |
| A Reason To Leave · electric piano and Rhodes | 0.02 | 4.89 | 0.07 |
| All Souls Moon · electric piano and Wurlitzer | 1.26 | 6.67 | 0.00 |
| Queen's Light · two electric pianos | −0.01 | 12.45 | 0.01 |

The median matched improvement over HTDemucs was **5.41 dB**. Banquet used
one independent electric-keys query per song and a three-second inference
hop. Those scores characterize that query and inference configuration.
The digital-piano checkpoint's weight terms remain unresolved. The
tests use labeled electric-keys groups rather than individual instrument
outputs within each group.
