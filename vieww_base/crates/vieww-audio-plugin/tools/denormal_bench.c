/* Times a CLAP plugin's process() alone on normal versus subnormal input.
 * clap-validator's process-audio-denormals test times its own buffer filling
 * (random subnormals generated without flush-to-zero) inside the measured
 * loop, so for a fast plugin its ratio mostly measures the validator. This
 * isolates the plugin and fails if subnormal input costs over 1.5x.
 * Usage: denormal_bench <plugin>   (Linux/macOS)
 */
#define _POSIX_C_SOURCE 199309L
#include <clap/clap.h>
#include <dlfcn.h>
#include <float.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
static const void *gx(const clap_host_t *h, const char *i) { (void)h; (void)i; return NULL; }
static void rq(const clap_host_t *h) { (void)h; }
static const clap_host_t host = {CLAP_VERSION_INIT, NULL, "b", "b", "b", "1", gx, rq, rq, rq};
static uint32_t sz(const clap_input_events_t *l) { (void)l; return 0; }
static const clap_event_header_t *gt(const clap_input_events_t *l, uint32_t i) { (void)l; (void)i; return NULL; }
static double now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec + t.tv_nsec * 1e-9; }
int main(int c, char **v) {
  void *lib = dlopen(v[1], RTLD_NOW); const clap_plugin_entry_t *e = dlsym(lib, "clap_entry");
  e->init(v[1]); const clap_plugin_factory_t *f = e->get_factory(CLAP_PLUGIN_FACTORY_ID);
  const clap_plugin_t *p = f->create_plugin(f, &host, f->get_plugin_descriptor(f, 0)->id);
  p->init(p); p->activate(p, 48000, 1, 512); p->start_processing(p);
  static float in[2][512], out[2][512]; float *ip[2] = {in[0], in[1]}, *op[2] = {out[0], out[1]};
  clap_audio_buffer_t ib = {ip, NULL, 2, 0, 0}, ob = {op, NULL, 2, 0, 0};
  clap_input_events_t ie = {NULL, sz, gt};
  clap_process_t pr = {0, 512, NULL, &ib, &ob, 1, 1, &ie, NULL};
  double t[2];
  for (int mode = 0; mode < 2; mode++) {
    for (int ch = 0; ch < 2; ch++) for (int i = 0; i < 512; i++)
      in[ch][i] = mode ? (float)((rand() / (double)RAND_MAX) * 2 - 1) * FLT_MIN * 0.5f
                       : (float)((rand() / (double)RAND_MAX) * 2 - 1);
    for (int k = 0; k < 200; k++) p->process(p, &pr);
    double t0 = now();
    for (int k = 0; k < 20000; k++) p->process(p, &pr);
    t[mode] = (now() - t0) / 20000 * 1e6;
    printf("  %s input: %.3f us/block\n", mode ? "subnormal" : "normal   ", t[mode]);
  }
  if (t[1] > 1.5 * t[0]) { fprintf(stderr, "subnormal input is %.2fx slower\n", t[1] / t[0]); return 1; }
  return 0;
}
