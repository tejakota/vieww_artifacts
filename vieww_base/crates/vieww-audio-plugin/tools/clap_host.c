/* A CLAP host in C, compiled against the official CLAP headers, that loads a
 * plugin library and exercises every part of the ABI the vieww wrapper
 * implements. It is the proof that the Rust transcription in src/clap/sys.rs
 * is the ABI rather than merely self-consistent.
 *
 * Usage: clap_host <plugin.so|.dylib|.dll>      (exit status 0 = all passed)
 */
#include <clap/clap.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#define LOAD(p) ((void *)LoadLibraryA(p))
#define SYM(h, s) ((void *)GetProcAddress((HMODULE)(h), s))
#define CLOSE(h) FreeLibrary((HMODULE)(h))
#else
#include <dlfcn.h>
#define LOAD(p) dlopen(p, RTLD_NOW | RTLD_LOCAL)
#define SYM(h, s) dlsym(h, s)
#define CLOSE(h) dlclose(h)
#endif

static int failures = 0;
#define CHECK(cond, ...)                                                     \
   do {                                                                      \
      if (!(cond)) {                                                         \
         fprintf(stderr, "FAIL %s:%d: ", __FILE__, __LINE__);                \
         fprintf(stderr, __VA_ARGS__);                                       \
         fputc('\n', stderr);                                                \
         failures++;                                                         \
      }                                                                      \
   } while (0)

/* ---- host ---------------------------------------------------------------*/
static const void *host_get_extension(const clap_host_t *h, const char *id) {
   (void)h; (void)id; return NULL;
}
static void host_request(const clap_host_t *h) { (void)h; }
static const clap_host_t host = {
   CLAP_VERSION_INIT, NULL, "c-test-host", "vieww", "https://vieww.dev", "1.0",
   host_get_extension, host_request, host_request, host_request,
};

/* ---- event lists --------------------------------------------------------*/
typedef struct { const clap_event_header_t *ev[16]; uint32_t n; } list_t;
static uint32_t list_size(const clap_input_events_t *l) { return ((list_t *)l->ctx)->n; }
static const clap_event_header_t *list_get(const clap_input_events_t *l, uint32_t i) {
   list_t *x = (list_t *)l->ctx;
   return i < x->n ? x->ev[i] : NULL;
}
static bool out_push(const clap_output_events_t *l, const clap_event_header_t *e) {
   (void)l; (void)e; return true;
}

/* ---- streams ------------------------------------------------------------*/
typedef struct { unsigned char data[65536]; size_t len, pos; } mem_t;
static int64_t mem_write(const clap_ostream_t *s, const void *b, uint64_t n) {
   mem_t *m = (mem_t *)s->ctx;
   if (m->len + n > sizeof m->data) return -1;
   /* accept at most 7 bytes per call: a plugin must loop on short writes */
   if (n > 7) n = 7;
   memcpy(m->data + m->len, b, n); m->len += n; return (int64_t)n;
}
static int64_t mem_read(const clap_istream_t *s, void *b, uint64_t n) {
   mem_t *m = (mem_t *)s->ctx;
   size_t k = m->len - m->pos;
   if (k > n) k = n;
   if (k > 5) k = 5; /* short reads too */
   memcpy(b, m->data + m->pos, k); m->pos += k; return (int64_t)k;
}

static clap_event_header_t header(uint32_t size, uint32_t time, uint16_t type) {
   clap_event_header_t h = {size, time, CLAP_CORE_EVENT_SPACE_ID, type, 0};
   return h;
}

/* Run one block; returns the process status. */
static clap_process_status run(const clap_plugin_t *p, uint32_t in_ch, uint32_t out_ch,
                               float in[][512], float out[][512], uint32_t frames,
                               list_t *events, const clap_event_transport_t *transport) {
   float *ip[8], *op[8];
   for (uint32_t c = 0; c < in_ch; c++) ip[c] = in[c];
   for (uint32_t c = 0; c < out_ch; c++) op[c] = out[c];
   clap_audio_buffer_t ib = {ip, NULL, in_ch, 0, 0}, ob = {op, NULL, out_ch, 0, 0};
   clap_input_events_t ie = {events, list_size, list_get};
   clap_output_events_t oe = {NULL, out_push};
   clap_process_t pr = {0, frames, transport, in_ch ? &ib : NULL, &ob, in_ch ? 1u : 0u, 1, &ie, &oe};
   return p->process(p, &pr);
}

static void test_plugin(const clap_plugin_factory_t *f, const clap_plugin_descriptor_t *d) {
   printf("  plugin %s \"%s\" v%s by %s\n", d->id, d->name, d->version, d->vendor);
   CHECK(clap_version_is_compatible(d->clap_version), "descriptor version");
   int instrument = 0;
   for (const char *const *ft = d->features; ft && *ft; ft++) {
      printf("    feature: %s\n", *ft);
      if (!strcmp(*ft, CLAP_PLUGIN_FEATURE_INSTRUMENT)) instrument = 1;
   }

   const clap_plugin_t *p = f->create_plugin(f, &host, d->id);
   CHECK(p != NULL, "create_plugin");
   if (!p) return;
   CHECK(p->desc == d || !strcmp(p->desc->id, d->id), "plugin desc");
   CHECK(p->init(p), "init");

   const clap_plugin_params_t *params = p->get_extension(p, CLAP_EXT_PARAMS);
   const clap_plugin_state_t *state = p->get_extension(p, CLAP_EXT_STATE);
   const clap_plugin_audio_ports_t *ports = p->get_extension(p, CLAP_EXT_AUDIO_PORTS);
   const clap_plugin_note_ports_t *notes = p->get_extension(p, CLAP_EXT_NOTE_PORTS);
   const clap_plugin_latency_t *latency = p->get_extension(p, CLAP_EXT_LATENCY);
   CHECK(params && state && ports && latency, "core extensions present");
   CHECK(p->get_extension(p, "clap.does-not-exist") == NULL, "unknown extension is NULL");
   CHECK(!instrument || notes, "instrument has note ports");
   if (!params || !state || !ports) return;

   /* Ports. */
   uint32_t in_ch = 0, out_ch = 0;
   for (int is_in = 0; is_in < 2; is_in++) {
      uint32_t n = ports->count(p, is_in);
      for (uint32_t i = 0; i < n; i++) {
         clap_audio_port_info_t info;
         memset(&info, 0xAB, sizeof info);
         CHECK(ports->get(p, i, is_in, &info), "audio port get");
         printf("    %s port %u \"%s\" %u ch type=%s main=%d\n", is_in ? "input" : "output",
                info.id, info.name, info.channel_count, info.port_type ? info.port_type : "(none)",
                (info.flags & CLAP_AUDIO_PORT_IS_MAIN) != 0);
         CHECK(info.in_place_pair == CLAP_INVALID_ID, "no in-place pair");
         if (is_in) in_ch = info.channel_count; else out_ch = info.channel_count;
      }
      clap_audio_port_info_t dummy;
      CHECK(!ports->get(p, n, is_in, &dummy), "out-of-range port refused");
   }
   CHECK(out_ch >= 1 && out_ch <= 8, "output channels");
   if (notes) {
      clap_note_port_info_t ni;
      CHECK(notes->count(p, true) == 1, "one note input");
      CHECK(notes->count(p, false) == 0, "no note output");
      CHECK(notes->get(p, 0, true, &ni), "note port get");
      CHECK(ni.supported_dialects & CLAP_NOTE_DIALECT_CLAP, "CLAP dialect");
      CHECK(ni.supported_dialects & CLAP_NOTE_DIALECT_MIDI, "MIDI dialect");
   }

   /* Parameters: info, values, text round trips. */
   uint32_t np = params->count(p);
   CHECK(np > 0, "has parameters");
   clap_id first = CLAP_INVALID_ID;
   double first_min = 0, first_max = 0;
   for (uint32_t i = 0; i < np; i++) {
      clap_param_info_t info;
      memset(&info, 0xCD, sizeof info);
      CHECK(params->get_info(p, i, &info), "param get_info");
      double v = NAN;
      CHECK(params->get_value(p, info.id, &v), "param get_value");
      char text[64];
      CHECK(params->value_to_text(p, info.id, v, text, sizeof text), "value_to_text");
      double back = NAN;
      CHECK(params->text_to_value(p, info.id, text, &back), "text_to_value(\"%s\")", text);
      double tol = fabs(info.max_value - info.min_value) * 1e-3 + 1e-9;
      CHECK(fabs(back - v) <= tol, "text round trip %s: %g -> \"%s\" -> %g", info.name, v, text, back);
      CHECK(info.min_value <= info.default_value && info.default_value <= info.max_value, "default in range");
      CHECK(fabs(v - info.default_value) < 1e-9, "starts at default");
      printf("    param %u \"%s\" [%g, %g] = %s%s%s\n", info.id, info.name, info.min_value,
             info.max_value, text, (info.flags & CLAP_PARAM_IS_STEPPED) ? " stepped" : "",
             (info.flags & CLAP_PARAM_IS_AUTOMATABLE) ? " automatable" : "");
      if (i == 0) { first = info.id; first_min = info.min_value; first_max = info.max_value; }
   }
   clap_param_info_t junk;
   CHECK(!params->get_info(p, np, &junk), "out-of-range param refused");
   double junkv;
   CHECK(!params->get_value(p, 0xDEADBEEF, &junkv), "unknown param refused");
   char small[4];
   CHECK(params->value_to_text(p, first, first_max, small, sizeof small), "tiny buffer");
   CHECK(strlen(small) < sizeof small, "tiny buffer stays terminated");

   /* Flush outside processing. */
   double target = first_min + (first_max - first_min) * 0.25;
   clap_event_param_value_t pv = {header(sizeof pv, 0, CLAP_EVENT_PARAM_VALUE), first, NULL, -1, -1, -1, -1, target};
   list_t l = {{&pv.header}, 1};
   clap_input_events_t ie = {&l, list_size, list_get};
   clap_output_events_t oe = {NULL, out_push};
   params->flush(p, &ie, &oe);
   double now = NAN;
   params->get_value(p, first, &now);
   CHECK(fabs(now - target) < 1e-9, "flush applied: %g vs %g", now, target);

   /* State save → load into a second instance. */
   mem_t *m = calloc(1, sizeof *m);
   clap_ostream_t os = {m, mem_write};
   CHECK(state->save(p, &os), "state save");
   CHECK(m->len > 0, "state non-empty");
   const clap_plugin_t *q = f->create_plugin(f, &host, d->id);
   q->init(q);
   const clap_plugin_state_t *qs = q->get_extension(q, CLAP_EXT_STATE);
   const clap_plugin_params_t *qp = q->get_extension(q, CLAP_EXT_PARAMS);
   clap_istream_t is = {m, mem_read};
   CHECK(qs->load(q, &is), "state load");
   double qv = NAN;
   qp->get_value(q, first, &qv);
   CHECK(fabs(qv - target) < 1e-9, "state restored: %g vs %g", qv, target);
   mem_t bad = {{1, 2, 3}, 3, 0};
   clap_istream_t bis = {&bad, mem_read};
   CHECK(!qs->load(q, &bis), "garbage state refused");
   q->destroy(q);
   free(m);

   /* Processing. */
   CHECK(p->activate(p, 48000.0, 1, 512), "activate");
   printf("    latency %u samples\n", latency->get(p));
   CHECK(p->start_processing(p), "start_processing");
   static float in[8][512], out[8][512];
   for (uint32_t c = 0; c < 8; c++)
      for (int i = 0; i < 512; i++) in[c][i] = 0.5f * sinf(i * 0.05f + c);
   clap_event_transport_t tr;
   memset(&tr, 0, sizeof tr);
   tr.header = header(sizeof tr, 0, CLAP_EVENT_TRANSPORT);
   tr.flags = CLAP_TRANSPORT_HAS_TEMPO | CLAP_TRANSPORT_IS_PLAYING | CLAP_TRANSPORT_HAS_BEATS_TIMELINE;
   tr.tempo = 120.0;
   tr.song_pos_beats = 4 * CLAP_BEATTIME_FACTOR;
   clap_event_note_t on = {header(sizeof on, 16, CLAP_EVENT_NOTE_ON), -1, 0, 0, 60, 0.9};
   clap_event_midi_t midi = {header(sizeof midi, 32, CLAP_EVENT_MIDI), 0, {0x90, 67, 100}};
   clap_event_param_value_t pv2 = {header(sizeof pv2, 200, CLAP_EVENT_PARAM_VALUE), first, NULL, -1, -1, -1, -1, first_min + (first_max - first_min) * 0.5};
   list_t ev = {{&on.header, &midi.header, &pv2.header}, 3};
   memset(out, 0, sizeof out);
   clap_process_status st = run(p, in_ch, out_ch, in, out, 512, &ev, &tr);
   CHECK(st != CLAP_PROCESS_ERROR, "process status %d", st);
   double energy = 0;
   int finite = 1;
   for (uint32_t c = 0; c < out_ch; c++)
      for (int i = 0; i < 512; i++) { energy += out[c][i] * out[c][i]; finite &= isfinite(out[c][i]); }
   CHECK(finite, "output finite");
   CHECK(energy > 1e-4, "output has signal (energy %g)", energy);
   if (instrument) {
      for (int i = 0; i < 16; i++) CHECK(out[0][i] == 0.0f, "silent before the note at frame 16");
   }
   params->get_value(p, first, &now);
   CHECK(fabs(now - (first_min + (first_max - first_min) * 0.5)) < 1e-9, "in-block automation applied");
   /* Many blocks of varying size, including 1 frame. */
   list_t none = {{0}, 0};
   uint32_t sizes[] = {1, 7, 64, 511, 512, 3};
   for (unsigned k = 0; k < sizeof sizes / sizeof *sizes; k++) {
      st = run(p, in_ch, out_ch, in, out, sizes[k], &none, NULL);
      CHECK(st != CLAP_PROCESS_ERROR, "block of %u", sizes[k]);
   }
   p->reset(p);
   p->stop_processing(p);
   p->deactivate(p);
   /* Reactivate at another rate. */
   CHECK(p->activate(p, 96000.0, 32, 256), "reactivate");
   CHECK(p->start_processing(p), "restart");
   st = run(p, in_ch, out_ch, in, out, 256, &none, NULL);
   CHECK(st != CLAP_PROCESS_ERROR, "process after reactivation");
   p->stop_processing(p);
   p->deactivate(p);
   p->destroy(p);
}

int main(int argc, char **argv) {
   if (argc != 2) { fprintf(stderr, "usage: %s plugin\n", argv[0]); return 2; }
   void *lib = LOAD(argv[1]);
   if (!lib) { fprintf(stderr, "cannot load %s\n", argv[1]); return 1; }
   const clap_plugin_entry_t *entry = SYM(lib, "clap_entry");
   CHECK(entry != NULL, "clap_entry exported");
   if (!entry) return 1;
   CHECK(clap_version_is_compatible(entry->clap_version), "entry version");
   CHECK(entry->init(argv[1]), "entry init");
   CHECK(entry->get_factory("clap.no-such-factory") == NULL, "unknown factory is NULL");
   const clap_plugin_factory_t *f = entry->get_factory(CLAP_PLUGIN_FACTORY_ID);
   CHECK(f != NULL, "plugin factory");
   if (!f) return 1;
   uint32_t n = f->get_plugin_count(f);
   printf("%s: %u plugin(s)\n", argv[1], n);
   CHECK(n > 0, "at least one plugin");
   CHECK(f->get_plugin_descriptor(f, n) == NULL, "out-of-range descriptor is NULL");
   CHECK(f->create_plugin(f, &host, "no.such.plugin") == NULL, "unknown id is NULL");
   for (uint32_t i = 0; i < n; i++) test_plugin(f, f->get_plugin_descriptor(f, i));
   entry->deinit();
   CLOSE(lib);
   if (failures) { fprintf(stderr, "%d check(s) failed\n", failures); return 1; }
   printf("all CLAP checks passed\n");
   return 0;
}
