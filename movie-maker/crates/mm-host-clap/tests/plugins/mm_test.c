/*
 * mm-host-clap のテストに使う、最小の CLAP のプラグイン。1 つの .clap に 2 つのプラグインを持つ。
 * - jp.durun.mm.test.gain: 出力 = 入力 x gain。パラメータの値はイベントの時刻（サンプル）から効く。
 *   crash が 0.5 以上になるとアクセス違反で落ち、sleep_ms があればブロックごとにその時間だけ眠る。
 * - jp.durun.mm.test.delay: 1 サンプルの遅延。reset で遅延の中身を 0 に戻す。
 * どちらもステレオの入力 1 つ、出力 1 つ。標準出力への printf（応答の JSON に混ざらないこと）も試す。
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <clap/clap.h>

#ifdef _WIN32
#include <windows.h>
static void sleep_ms(int ms) { Sleep((DWORD)ms); }
#else
#include <unistd.h>
static void sleep_ms(int ms) { usleep((useconds_t)ms * 1000); }
#endif

#define CHANNELS 2

enum { KIND_GAIN = 0, KIND_DELAY = 1, KIND_COUNT = 2 };
enum { P_GAIN = 1, P_CRASH = 2, P_SLEEP = 3 };

static const char *const FEATURES[] = {CLAP_PLUGIN_FEATURE_AUDIO_EFFECT,
                                       CLAP_PLUGIN_FEATURE_STEREO, NULL};

static const clap_plugin_descriptor_t DESCS[KIND_COUNT] = {
    {CLAP_VERSION_INIT, "jp.durun.mm.test.gain", "MM Test Gain", "movie-maker", "", "", "",
     "1.0.0", "Multiplies by gain", FEATURES},
    {CLAP_VERSION_INIT, "jp.durun.mm.test.delay", "MM Test Delay", "movie-maker", "", "", "",
     "1.0.0", "Delays by one sample", FEATURES},
};

typedef struct {
  clap_id id;
  const char *name;
  uint32_t flags;
  double min, max, def;
} param_def;

static const param_def GAIN_PARAMS[] = {
    {P_GAIN, "gain", CLAP_PARAM_IS_AUTOMATABLE, 0.0, 4.0, 1.0},
    {P_CRASH, "crash", CLAP_PARAM_IS_STEPPED, 0.0, 1.0, 0.0},
    {P_SLEEP, "sleep_ms", CLAP_PARAM_IS_STEPPED, 0.0, 60000.0, 0.0},
};
#define GAIN_PARAM_COUNT (sizeof(GAIN_PARAMS) / sizeof(GAIN_PARAMS[0]))

typedef struct {
  clap_plugin_t plugin;
  int kind;
  double gain, crash, sleep;
  float last[CHANNELS];
} mm_plugin;

static mm_plugin *self_of(const clap_plugin_t *plugin) { return (mm_plugin *)plugin->plugin_data; }

static void crash_now(void) {
  volatile int *p = NULL;
  *p = 1;
}

static void set_param(mm_plugin *p, clap_id id, double value) {
  switch (id) {
  case P_GAIN: p->gain = value; break;
  case P_CRASH: p->crash = value; break;
  case P_SLEEP: p->sleep = value; break;
  default: break;
  }
}

static int get_param(const mm_plugin *p, clap_id id, double *value) {
  switch (id) {
  case P_GAIN: *value = p->gain; return 1;
  case P_CRASH: *value = p->crash; return 1;
  case P_SLEEP: *value = p->sleep; return 1;
  default: return 0;
  }
}

static void apply_event(mm_plugin *p, const clap_event_header_t *h) {
  if (h->space_id != CLAP_CORE_EVENT_SPACE_ID || h->type != CLAP_EVENT_PARAM_VALUE) return;
  const clap_event_param_value_t *ev = (const clap_event_param_value_t *)h;
  set_param(p, ev->param_id, ev->value);
  if (p->crash >= 0.5) crash_now();
}

/* ---- clap_plugin ---- */

static bool p_init(const clap_plugin_t *plugin) {
  (void)plugin;
  printf("mm-test: init\n");
  fflush(stdout);
  return true;
}

static void p_destroy(const clap_plugin_t *plugin) { free(self_of(plugin)); }

static bool p_activate(const clap_plugin_t *plugin, double sr, uint32_t min, uint32_t max) {
  (void)plugin; (void)sr; (void)min; (void)max;
  return true;
}

static void p_deactivate(const clap_plugin_t *plugin) { (void)plugin; }
static bool p_start(const clap_plugin_t *plugin) { (void)plugin; return true; }
static void p_stop(const clap_plugin_t *plugin) { (void)plugin; }

static void p_reset(const clap_plugin_t *plugin) {
  mm_plugin *p = self_of(plugin);
  memset(p->last, 0, sizeof(p->last));
}

static float process_sample(mm_plugin *p, int c, float x) {
  if (p->kind == KIND_GAIN) return (float)(x * p->gain);
  float y = p->last[c];
  p->last[c] = x;
  return y;
}

static clap_process_status p_process(const clap_plugin_t *plugin, const clap_process_t *proc) {
  mm_plugin *p = self_of(plugin);
  if (proc->audio_inputs_count < 1 || proc->audio_outputs_count < 1) return CLAP_PROCESS_ERROR;
  float **in = proc->audio_inputs[0].data32;
  float **out = proc->audio_outputs[0].data32;
  uint32_t nev = proc->in_events->size(proc->in_events);
  uint32_t ei = 0;
  for (uint32_t i = 0; i < proc->frames_count; ++i) {
    for (; ei < nev; ++ei) {
      const clap_event_header_t *h = proc->in_events->get(proc->in_events, ei);
      if (h->time > i) break;
      apply_event(p, h);
    }
    for (int c = 0; c < CHANNELS; ++c) out[c][i] = process_sample(p, c, in[c][i]);
  }
  for (; ei < nev; ++ei) apply_event(p, proc->in_events->get(proc->in_events, ei));
  if (p->sleep > 0.0) sleep_ms((int)p->sleep);
  return CLAP_PROCESS_CONTINUE;
}

/* ---- audio-ports ---- */

static uint32_t ports_count(const clap_plugin_t *plugin, bool is_input) {
  (void)plugin; (void)is_input;
  return 1;
}

static bool ports_get(const clap_plugin_t *plugin, uint32_t index, bool is_input,
                      clap_audio_port_info_t *info) {
  (void)plugin;
  if (index != 0) return false;
  info->id = 0;
  snprintf(info->name, sizeof(info->name), "%s", is_input ? "in" : "out");
  info->flags = CLAP_AUDIO_PORT_IS_MAIN;
  info->channel_count = CHANNELS;
  info->port_type = CLAP_PORT_STEREO;
  info->in_place_pair = CLAP_INVALID_ID;
  return true;
}

static const clap_plugin_audio_ports_t AUDIO_PORTS = {ports_count, ports_get};

/* ---- params ---- */

static uint32_t params_count(const clap_plugin_t *plugin) {
  return self_of(plugin)->kind == KIND_GAIN ? (uint32_t)GAIN_PARAM_COUNT : 0;
}

static bool params_info(const clap_plugin_t *plugin, uint32_t index, clap_param_info_t *info) {
  if (index >= params_count(plugin)) return false;
  const param_def *d = &GAIN_PARAMS[index];
  memset(info, 0, sizeof(*info));
  info->id = d->id;
  info->flags = d->flags;
  snprintf(info->name, sizeof(info->name), "%s", d->name);
  info->min_value = d->min;
  info->max_value = d->max;
  info->default_value = d->def;
  return true;
}

static bool params_value(const clap_plugin_t *plugin, clap_id id, double *value) {
  return get_param(self_of(plugin), id, value) != 0;
}

static bool params_to_text(const clap_plugin_t *plugin, clap_id id, double value, char *out,
                           uint32_t size) {
  (void)plugin; (void)id;
  snprintf(out, size, "%g", value);
  return true;
}

static bool params_from_text(const clap_plugin_t *plugin, clap_id id, const char *text,
                             double *value) {
  (void)plugin; (void)id;
  *value = strtod(text, NULL);
  return true;
}

static void params_flush(const clap_plugin_t *plugin, const clap_input_events_t *in,
                         const clap_output_events_t *out) {
  (void)out;
  mm_plugin *p = self_of(plugin);
  uint32_t n = in->size(in);
  for (uint32_t i = 0; i < n; ++i) apply_event(p, in->get(in, i));
}

static const clap_plugin_params_t PARAMS = {params_count, params_info, params_value,
                                            params_to_text, params_from_text, params_flush};

static const void *p_extension(const clap_plugin_t *plugin, const char *id) {
  (void)plugin;
  if (!strcmp(id, CLAP_EXT_AUDIO_PORTS)) return &AUDIO_PORTS;
  if (!strcmp(id, CLAP_EXT_PARAMS)) return &PARAMS;
  return NULL;
}

static void p_on_main_thread(const clap_plugin_t *plugin) { (void)plugin; }

/* ---- factory と entry ---- */

static uint32_t f_count(const clap_plugin_factory_t *f) {
  (void)f;
  return KIND_COUNT;
}

static const clap_plugin_descriptor_t *f_descriptor(const clap_plugin_factory_t *f, uint32_t i) {
  (void)f;
  return i < KIND_COUNT ? &DESCS[i] : NULL;
}

static const clap_plugin_t *f_create(const clap_plugin_factory_t *f, const clap_host_t *host,
                                     const char *plugin_id) {
  (void)f; (void)host;
  for (int k = 0; k < KIND_COUNT; ++k) {
    if (strcmp(plugin_id, DESCS[k].id) != 0) continue;
    mm_plugin *p = (mm_plugin *)calloc(1, sizeof(mm_plugin));
    if (!p) return NULL;
    p->kind = k;
    p->gain = 1.0;
    p->plugin.desc = &DESCS[k];
    p->plugin.plugin_data = p;
    p->plugin.init = p_init;
    p->plugin.destroy = p_destroy;
    p->plugin.activate = p_activate;
    p->plugin.deactivate = p_deactivate;
    p->plugin.start_processing = p_start;
    p->plugin.stop_processing = p_stop;
    p->plugin.reset = p_reset;
    p->plugin.process = p_process;
    p->plugin.get_extension = p_extension;
    p->plugin.on_main_thread = p_on_main_thread;
    return &p->plugin;
  }
  return NULL;
}

static const clap_plugin_factory_t FACTORY = {f_count, f_descriptor, f_create};

static bool entry_init(const char *path) {
  (void)path;
  return true;
}

static void entry_deinit(void) {}

static const void *entry_factory(const char *id) {
  return strcmp(id, CLAP_PLUGIN_FACTORY_ID) == 0 ? &FACTORY : NULL;
}

CLAP_EXPORT const clap_plugin_entry_t clap_entry = {CLAP_VERSION_INIT, entry_init, entry_deinit,
                                                    entry_factory};
