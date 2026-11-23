/*
 * daw_stretch_abi.h — Minimal C ABI for swappable real-time time-stretch
 * and pitch-shift engines loaded dynamically by FastTrackStudio DAW.
 *
 * Any dynamic library (.so / .dylib / .dll) exporting:
 *   const DawStretchVTable* daw_stretch_vtable(void);
 * can be discovered and hot-swapped by the DAW engine without recompiling.
 */
#ifndef DAW_STRETCH_ABI_H
#define DAW_STRETCH_ABI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define DAW_STRETCH_ABI_VERSION 1u

/* Capability bitflags */
#define DAW_STRETCH_CAP_TIME_STRETCH      (1u << 0)
#define DAW_STRETCH_CAP_PITCH_SHIFT       (1u << 1)
#define DAW_STRETCH_CAP_FORMANT_PRESERVE  (1u << 2)
#define DAW_STRETCH_CAP_STEREO_PHASE_LOCK (1u << 3)
#define DAW_STRETCH_CAP_TRANSIENT_AWARE   (1u << 4)

/*
 * Pull callback supplied by the host DAW.
 * Fills `left[0..frames]` and `right[0..frames]` with source audio starting at
 * `start_frame` (out-of-range frames are zero-filled by the host).
 */
typedef void (*DawStretchReadFn)(
    void* ctx,
    int64_t start_frame,
    float* left,
    float* right,
    size_t frames
);

typedef struct DawStretchSource {
    void* ctx;
    DawStretchReadFn read;
} DawStretchSource;

typedef struct DawStretchInfo {
    uint32_t abi_version;
    uint32_t capabilities;
    const char* id;          /* stable reverse-dns / slug id, e.g. "fts.stretch.pv" */
    const char* name;        /* human-readable name */
    const char* vendor;      /* vendor / author */
    const char* version;     /* semver string */
} DawStretchInfo;

typedef struct DawStretchVTable {
    uint32_t abi_version;

    /* Static metadata for this engine. */
    DawStretchInfo (*info)(void);

    /* Allocate a stereo stretcher voice for `sample_rate`. Returns opaque handle. */
    void* (*create)(double sample_rate);

    /* Free a voice created by `create`. */
    void (*destroy)(void* handle);

    /*
     * Seek to `source_frame` and prime internal overlap buffers so the next
     * `render()` call outputs audio starting right at `source_frame`.
     * `ratio` = input frames per output frame (0.5 = half speed / 2x longer).
     * `pitch_semitones` = pitch transposition in semitones (0.0 = unchanged).
     */
    void (*seek)(
        void* handle,
        double source_frame,
        double ratio,
        float pitch_semitones,
        DawStretchSource source
    );

    /*
     * Render `frames` stereo output frames into `out_left` and `out_right`.
     * `ratio` and `pitch_semitones` may vary block-to-block.
     */
    void (*render)(
        void* handle,
        float* out_left,
        float* out_right,
        size_t frames,
        double ratio,
        float pitch_semitones,
        DawStretchSource source
    );

    /* Reported algorithmic latency in frames (0 if compensated by seek). */
    size_t (*latency_frames)(const void* handle);
} DawStretchVTable;

/* Symbol every stretch engine plugin exports. */
const DawStretchVTable* daw_stretch_vtable(void);

#ifdef __cplusplus
}
#endif

#endif /* DAW_STRETCH_ABI_H */
