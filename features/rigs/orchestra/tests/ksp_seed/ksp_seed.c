// ksp_seed.clap — makes Kontakt's KSP random() reproducible in a render.
//
// Kontakt's KSP random() draws from libc rand() (three calls per draw), which
// Kontakt seeds with srand(time(NULL)). Placed in a render's FX chain, this
// plugin rebinds Kontakt's imports of rand() and srand() (its __got / symbol
// pointer entries) to wrappers with their own Park–Miller state — the same
// generator as macOS libc. At the first block the transport plays it sets that
// state to KSP_SEED; from then on Kontakt's srand() calls are ignored, so the
// draws depend only on the seed, as KONTRA's (KONTRA_KSP_SEED) do.
//
// KSP_SEED unset: the plugin does nothing. KSP_SEED_LOG=<file>: every
// rand()/srand() call Kontakt makes after the rebind is logged with its caller
// (offset in the Kontakt image) and the value returned.
// MIDI and audio pass through (no ports: REAPER forwards both).
#include <clap/clap.h>
#include <mach-o/dyld.h>
#include <mach-o/loader.h>
#include <mach-o/nlist.h>
#include <mach/mach.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>

static unsigned g_state = 1;
static int g_pinned = 0;
static uintptr_t g_kontakt_base = 0;
static FILE *g_log = NULL;
static unsigned long g_calls = 0;

static int park_miller(void) {
    long ctx = (long)g_state;
    if (ctx == 0) ctx = 123459876;
    long hi = ctx / 127773, lo = ctx % 127773;
    long x = 16807 * lo - 2836 * hi;
    if (x < 0) x += 0x7fffffff;
    g_state = (unsigned)x;
    return (int)x;
}

static int hook_rand(void) {
    int v = park_miller();
    if (g_log && g_calls < 200000) {
        uintptr_t ra = (uintptr_t)__builtin_return_address(0);
        fprintf(g_log, "rand %lu caller %#lx -> %d\n", g_calls, (unsigned long)(ra - g_kontakt_base), v);
    }
    g_calls++;
    return v;
}

static void hook_srand(unsigned seed) {
    if (g_log) {
        uintptr_t ra = (uintptr_t)__builtin_return_address(0);
        fprintf(g_log, "srand(%u) caller %#lx %s\n", seed, (unsigned long)(ra - g_kontakt_base),
                g_pinned ? "ignored" : "applied");
    }
    if (!g_pinned) g_state = seed;
}

// Rebind the symbol pointers named `_rand` / `_srand` in one image.
static int rebind_image(const struct mach_header_64 *mh, intptr_t slide) {
    const struct load_command *lc = (const void *)(mh + 1);
    const struct symtab_command *symtab = NULL;
    const struct dysymtab_command *dysym = NULL;
    const struct segment_command_64 *linkedit = NULL;
    for (uint32_t i = 0; i < mh->ncmds; i++) {
        if (lc->cmd == LC_SYMTAB) symtab = (const void *)lc;
        else if (lc->cmd == LC_DYSYMTAB) dysym = (const void *)lc;
        else if (lc->cmd == LC_SEGMENT_64 && strcmp(((const struct segment_command_64 *)lc)->segname, SEG_LINKEDIT) == 0)
            linkedit = (const void *)lc;
        lc = (const void *)((const char *)lc + lc->cmdsize);
    }
    if (!symtab || !dysym || !linkedit) return 0;
    uintptr_t le_base = (uintptr_t)slide + linkedit->vmaddr - linkedit->fileoff;
    const struct nlist_64 *syms = (const void *)(le_base + symtab->symoff);
    const char *strs = (const char *)(le_base + symtab->stroff);
    const uint32_t *indirect = (const void *)(le_base + dysym->indirectsymoff);
    int n = 0;
    lc = (const void *)(mh + 1);
    for (uint32_t i = 0; i < mh->ncmds; i++, lc = (const void *)((const char *)lc + lc->cmdsize)) {
        if (lc->cmd != LC_SEGMENT_64) continue;
        const struct segment_command_64 *seg = (const void *)lc;
        const struct section_64 *sect = (const void *)(seg + 1);
        for (uint32_t s = 0; s < seg->nsects; s++, sect++) {
            uint32_t type = sect->flags & SECTION_TYPE;
            if (type != S_LAZY_SYMBOL_POINTERS && type != S_NON_LAZY_SYMBOL_POINTERS) continue;
            void **ptrs = (void **)((uintptr_t)slide + sect->addr);
            uint32_t count = (uint32_t)(sect->size / sizeof(void *));
            for (uint32_t k = 0; k < count; k++) {
                uint32_t idx = indirect[sect->reserved1 + k];
                if (idx & (INDIRECT_SYMBOL_ABS | INDIRECT_SYMBOL_LOCAL)) continue;
                const char *name = strs + syms[idx].n_un.n_strx;
                void *to = NULL;
                if (strcmp(name, "_rand") == 0) to = (void *)hook_rand;
                else if (strcmp(name, "_srand") == 0) to = (void *)hook_srand;
                if (!to) continue;
                uintptr_t page = (uintptr_t)&ptrs[k] & ~(uintptr_t)(vm_page_size - 1);
                vm_protect(mach_task_self(), page, vm_page_size, 0, VM_PROT_READ | VM_PROT_WRITE | VM_PROT_COPY);
                ptrs[k] = to;
                n++;
            }
        }
    }
    return n;
}

static int rebind_kontakt(void) {
    int total = 0;
    for (uint32_t i = 0; i < _dyld_image_count(); i++) {
        const char *name = _dyld_get_image_name(i);
        if (!name || !strstr(name, "Kontakt")) continue;
        const struct mach_header_64 *mh = (const void *)_dyld_get_image_header(i);
        intptr_t slide = _dyld_get_image_vmaddr_slide(i);
        g_kontakt_base = (uintptr_t)slide + 0x0;  // image vmaddr 0 (MH_BUNDLE)
        int n = rebind_image(mh, slide);
        if (g_log) fprintf(g_log, "rebound %d pointers in %s (slide %#lx)\n", n, name, (unsigned long)slide);
        total += n;
    }
    return total;
}

typedef struct {
    clap_plugin_t plugin;
    int have_seed;
    unsigned seed;
    int was_playing;
    int rebound;
} seed_plugin;

static bool p_init(const clap_plugin_t *p) { (void)p; return true; }
static void p_destroy(const clap_plugin_t *p) { free(p->plugin_data); }
static bool p_activate(const clap_plugin_t *p, double sr, uint32_t minf, uint32_t maxf) {
    (void)sr; (void)minf; (void)maxf;
    seed_plugin *s = p->plugin_data;
    const char *v = getenv("KSP_SEED");
    s->have_seed = v && *v;
    s->seed = s->have_seed ? (unsigned)strtoul(v, NULL, 10) : 0;
    s->was_playing = 0;
    const char *log = getenv("KSP_SEED_LOG");
    if (s->have_seed && log && !g_log) {
        g_log = fopen(log, "w");
        if (g_log) setvbuf(g_log, NULL, _IOLBF, 0);
    }
    return true;
}
static void p_deactivate(const clap_plugin_t *p) { (void)p; if (g_log) fflush(g_log); }
static bool p_start(const clap_plugin_t *p) { (void)p; return true; }
static void p_stop(const clap_plugin_t *p) { (void)p; }
static void p_reset(const clap_plugin_t *p) { (void)p; }

static clap_process_status p_process(const clap_plugin_t *p, const clap_process_t *proc) {
    seed_plugin *s = p->plugin_data;
    if (!s->have_seed) return CLAP_PROCESS_CONTINUE;
    int playing = proc->transport && (proc->transport->flags & CLAP_TRANSPORT_IS_PLAYING);
    if (playing && !s->was_playing) {
        if (!s->rebound) s->rebound = rebind_kontakt() > 0;
        g_state = s->seed;
        g_pinned = 1;
        g_calls = 0;
        if (g_log) fprintf(g_log, "transport start: state = %u\n", s->seed);
    }
    s->was_playing = playing;
    return CLAP_PROCESS_CONTINUE;
}

static const void *p_ext(const clap_plugin_t *p, const char *id) { (void)p; (void)id; return NULL; }
static void p_main(const clap_plugin_t *p) { (void)p; }

static const char *features[] = {CLAP_PLUGIN_FEATURE_AUDIO_EFFECT, CLAP_PLUGIN_FEATURE_UTILITY, NULL};
static const clap_plugin_descriptor_t desc = {
    .clap_version = CLAP_VERSION_INIT,
    .id = "com.fts.ksp-seed",
    .name = "KSP Seed",
    .vendor = "FTS",
    .url = "",
    .manual_url = "",
    .support_url = "",
    .version = "1.1.0",
    .description = "Kontakt's rand()/srand() pinned to KSP_SEED at transport start, for reproducible renders",
    .features = features,
};

static const clap_plugin_t *create(const clap_plugin_factory_t *f, const clap_host_t *host, const char *id) {
    (void)f; (void)host;
    if (strcmp(id, desc.id) != 0) return NULL;
    seed_plugin *s = calloc(1, sizeof *s);
    s->plugin = (clap_plugin_t){
        .desc = &desc, .plugin_data = s, .init = p_init, .destroy = p_destroy,
        .activate = p_activate, .deactivate = p_deactivate, .start_processing = p_start,
        .stop_processing = p_stop, .reset = p_reset, .process = p_process,
        .get_extension = p_ext, .on_main_thread = p_main,
    };
    return &s->plugin;
}
static uint32_t count(const clap_plugin_factory_t *f) { (void)f; return 1; }
static const clap_plugin_descriptor_t *get_desc(const clap_plugin_factory_t *f, uint32_t i) {
    (void)f; return i == 0 ? &desc : NULL;
}
static const clap_plugin_factory_t factory = {.get_plugin_count = count, .get_plugin_descriptor = get_desc, .create_plugin = create};
static bool e_init(const char *path) { (void)path; return true; }
static void e_deinit(void) { if (g_log) fclose(g_log); g_log = NULL; }
static const void *e_factory(const char *id) {
    return strcmp(id, CLAP_PLUGIN_FACTORY_ID) == 0 ? &factory : NULL;
}
CLAP_EXPORT const clap_plugin_entry_t clap_entry = {
    .clap_version = CLAP_VERSION_INIT, .init = e_init, .deinit = e_deinit, .get_factory = e_factory,
};
