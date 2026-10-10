// A VST3 host in C++, compiled against Steinberg's VST3 interface headers
// only (no SDK helper classes), that loads a module and exercises every
// interface the vieww wrapper implements. The C++ compiler lays out the
// vtables, structs and IIDs here — so this proves the Rust transcription in
// src/vst3/sys.rs is the ABI.
//
// Usage: vst3_host <module.so|.vst3 binary>      (exit status 0 = all passed)
#define INIT_CLASS_IID
#include "pluginterfaces/base/ipluginbase.h"
#include "pluginterfaces/base/ibstream.h"
#include "pluginterfaces/vst/ivstcomponent.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "pluginterfaces/vst/ivstevents.h"
#include "pluginterfaces/vst/ivstparameterchanges.h"
#include "pluginterfaces/vst/ivstprocesscontext.h"
#include "pluginterfaces/vst/ivsthostapplication.h"
#include "pluginterfaces/vst/ivstmidicontrollers.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#ifdef _WIN32
#include <windows.h>
#define LOAD(p) ((void*)LoadLibraryA(p))
#define SYM(h, s) ((void*)GetProcAddress((HMODULE)(h), s))
#define CLOSE(h) FreeLibrary((HMODULE)(h))
#else
#include <dlfcn.h>
#define LOAD(p) dlopen(p, RTLD_NOW | RTLD_LOCAL)
#define SYM(h, s) dlsym(h, s)
#define CLOSE(h) dlclose(h)
#endif

using namespace Steinberg;
using namespace Steinberg::Vst;

static int failures = 0;
#define CHECK(cond, ...)                                                       \
    do {                                                                       \
        if (!(cond)) {                                                         \
            std::fprintf(stderr, "FAIL %s:%d: ", __FILE__, __LINE__);          \
            std::fprintf(stderr, __VA_ARGS__);                                 \
            std::fputc('\n', stderr);                                          \
            failures++;                                                        \
        }                                                                      \
    } while (0)

// A base for host objects: FUnknown with a counter that never frees (the
// host owns them on the stack or as members).
#define HOST_UNKNOWN(Iface)                                                         \
    uint32 refs = 1;                                                                \
    tresult PLUGIN_API queryInterface(const TUID iid, void** obj) override {        \
        if (FUnknownPrivate::iidEqual(iid, FUnknown::iid) ||                        \
            FUnknownPrivate::iidEqual(iid, Iface::iid)) {                           \
            addRef(); *obj = this; return kResultOk;                                \
        }                                                                           \
        *obj = nullptr; return kNoInterface;                                        \
    }                                                                               \
    uint32 PLUGIN_API addRef() override { return ++refs; }                          \
    uint32 PLUGIN_API release() override { return --refs; }

struct HostApp : IHostApplication {
    HOST_UNKNOWN(IHostApplication)
    tresult PLUGIN_API getName(String128 name) override {
        const char* n = "cpp-test-host";
        for (int i = 0; ; i++) { name[i] = (char16)n[i]; if (!n[i]) break; }
        return kResultOk;
    }
    tresult PLUGIN_API createInstance(TUID, TUID, void** obj) override { *obj = nullptr; return kResultFalse; }
};

struct Handler : IComponentHandler {
    HOST_UNKNOWN(IComponentHandler)
    int32 restarts = 0;
    tresult PLUGIN_API beginEdit(ParamID) override { return kResultOk; }
    tresult PLUGIN_API performEdit(ParamID, ParamValue) override { return kResultOk; }
    tresult PLUGIN_API endEdit(ParamID) override { return kResultOk; }
    tresult PLUGIN_API restartComponent(int32 flags) override { restarts |= flags; return kResultOk; }
};

// A stream that hands out at most `chunk` bytes per call, so plugins must
// loop on short reads and writes.
struct Stream : IBStream {
    HOST_UNKNOWN(IBStream)
    std::vector<unsigned char> data;
    int64 pos = 0;
    int32 chunk = 13;
    tresult PLUGIN_API read(void* buf, int32 n, int32* got) override {
        int64 k = std::min<int64>({(int64)n, (int64)data.size() - pos, (int64)chunk});
        if (k < 0) k = 0;
        std::memcpy(buf, data.data() + pos, (size_t)k);
        pos += k;
        if (got) *got = (int32)k;
        return kResultOk;
    }
    tresult PLUGIN_API write(void* buf, int32 n, int32* put) override {
        int32 k = std::min(n, chunk);
        if ((int64)data.size() < pos + k) data.resize((size_t)(pos + k));
        std::memcpy(data.data() + pos, buf, (size_t)k);
        pos += k;
        if (put) *put = k;
        return kResultOk;
    }
    tresult PLUGIN_API seek(int64 p, int32 mode, int64* r) override {
        int64 b = mode == kIBSeekSet ? 0 : mode == kIBSeekCur ? pos : (int64)data.size();
        pos = b + p;
        if (r) *r = pos;
        return kResultOk;
    }
    tresult PLUGIN_API tell(int64* p) override { if (p) *p = pos; return kResultOk; }
};

struct Queue : IParamValueQueue {
    HOST_UNKNOWN(IParamValueQueue)
    ParamID id = 0;
    std::vector<std::pair<int32, ParamValue>> points;
    ParamID PLUGIN_API getParameterId() override { return id; }
    int32 PLUGIN_API getPointCount() override { return (int32)points.size(); }
    tresult PLUGIN_API getPoint(int32 i, int32& off, ParamValue& v) override {
        if (i < 0 || i >= (int32)points.size()) return kInvalidArgument;
        off = points[(size_t)i].first; v = points[(size_t)i].second; return kResultOk;
    }
    tresult PLUGIN_API addPoint(int32, ParamValue, int32&) override { return kResultFalse; }
};

struct Changes : IParameterChanges {
    HOST_UNKNOWN(IParameterChanges)
    std::vector<Queue> queues;
    int32 PLUGIN_API getParameterCount() override { return (int32)queues.size(); }
    IParamValueQueue* PLUGIN_API getParameterData(int32 i) override {
        return i >= 0 && i < (int32)queues.size() ? &queues[(size_t)i] : nullptr;
    }
    IParamValueQueue* PLUGIN_API addParameterData(const ParamID&, int32&) override { return nullptr; }
};

struct Events : IEventList {
    HOST_UNKNOWN(IEventList)
    std::vector<Event> events;
    int32 PLUGIN_API getEventCount() override { return (int32)events.size(); }
    tresult PLUGIN_API getEvent(int32 i, Event& e) override {
        if (i < 0 || i >= (int32)events.size()) return kInvalidArgument;
        e = events[(size_t)i]; return kResultOk;
    }
    tresult PLUGIN_API addEvent(Event&) override { return kResultOk; }
};

static std::string narrow(const char16* s) {
    std::string out;
    for (; *s; s++) out += (char)(*s < 128 ? *s : '?');
    return out;
}

static void test_class(IPluginFactory* f, IPluginFactory2* f2, int32 idx, HostApp* host) {
    PClassInfo2 ci{};
    CHECK(f2->getClassInfo2(idx, &ci) == kResultOk, "getClassInfo2");
    std::printf("  class \"%s\" [%s] %s v%s (%s)\n", ci.name, ci.category, ci.subCategories, ci.version, ci.sdkVersion);
    CHECK(!std::strcmp(ci.category, kVstAudioEffectClass), "category");
    bool instrument = std::strncmp(ci.subCategories, "Instrument", 10) == 0;

    IComponent* comp = nullptr;
    CHECK(f->createInstance(ci.cid, IComponent::iid, (void**)&comp) == kResultOk && comp, "createInstance");
    if (!comp) return;
    CHECK(comp->initialize(host) == kResultOk, "initialize");

    IAudioProcessor* proc = nullptr;
    CHECK(comp->queryInterface(IAudioProcessor::iid, (void**)&proc) == kResultOk && proc, "IAudioProcessor");
    IEditController* ctrl = nullptr;
    CHECK(comp->queryInterface(IEditController::iid, (void**)&ctrl) == kResultOk && ctrl, "single-component controller");
    IProcessContextRequirements* req = nullptr;
    if (comp->queryInterface(IProcessContextRequirements::iid, (void**)&req) == kResultOk) {
        uint32 r = req->getProcessContextRequirements();
        CHECK(r & IProcessContextRequirements::kNeedTempo, "needs tempo");
        req->release();
    }
    void* nothing = (void*)1;
    CHECK(comp->queryInterface(IHostApplication::iid, &nothing) == kNoInterface && !nothing, "unknown iid refused");
    if (!proc || !ctrl) return;

    // Buses.
    int32 audio_in = comp->getBusCount(kAudio, kInput), audio_out = comp->getBusCount(kAudio, kOutput);
    int32 ev_in = comp->getBusCount(kEvent, kInput);
    CHECK(audio_out == 1, "one output bus");
    CHECK((ev_in == 1) == instrument, "event bus iff instrument");
    int32 in_ch = 0, out_ch = 0;
    for (int32 dir = 0; dir < 2; dir++) {
        int32 n = comp->getBusCount(kAudio, dir);
        for (int32 i = 0; i < n; i++) {
            BusInfo b{};
            CHECK(comp->getBusInfo(kAudio, dir, i, b) == kResultOk, "getBusInfo");
            std::printf("    %s bus \"%s\" %d ch\n", dir ? "output" : "input", narrow(b.name).c_str(), b.channelCount);
            (dir ? out_ch : in_ch) = b.channelCount;
            CHECK(b.flags & BusInfo::kDefaultActive, "default active");
            CHECK(comp->activateBus(kAudio, dir, i, true) == kResultOk, "activateBus");
        }
    }
    SpeakerArrangement sa = 0;
    CHECK(proc->getBusArrangement(kOutput, 0, sa) == kResultOk && sa == SpeakerArr::kStereo, "stereo out");
    SpeakerArrangement st = SpeakerArr::kStereo, mono = SpeakerArr::kMono;
    CHECK(proc->setBusArrangements(audio_in ? &st : nullptr, audio_in, &st, 1) == kResultOk, "accept own arrangement");
    CHECK(proc->setBusArrangements(audio_in ? &mono : nullptr, audio_in, &mono, 1) == kResultFalse, "refuse mono");
    CHECK(proc->canProcessSampleSize(kSample32) == kResultOk, "32-bit");
    CHECK(proc->canProcessSampleSize(kSample64) == kResultFalse, "no 64-bit");

    // Parameters.
    Handler handler;
    CHECK(ctrl->setComponentHandler(&handler) == kResultOk, "setComponentHandler");
    int32 np = ctrl->getParameterCount();
    CHECK(np > 0, "parameters");
    ParamID first = 0;
    int visible = 0;
    for (int32 i = 0; i < np; i++) {
        ParameterInfo pi{};
        CHECK(ctrl->getParameterInfo(i, pi) == kResultOk, "getParameterInfo");
        if (pi.flags & ParameterInfo::kIsHidden) continue;
        String128 text{};
        ParamValue v = ctrl->getParamNormalized(pi.id);
        CHECK(std::fabs(v - pi.defaultNormalizedValue) < 1e-9, "starts at default");
        CHECK(ctrl->getParamStringByValue(pi.id, v, text) == kResultOk, "string by value");
        ParamValue back = -1;
        CHECK(ctrl->getParamValueByString(pi.id, text, back) == kResultOk, "value by string");
        CHECK(std::fabs(back - v) < 2e-3, "text round trip");
        ParamValue plain = ctrl->normalizedParamToPlain(pi.id, 0.3);
        CHECK(std::fabs(ctrl->plainParamToNormalized(pi.id, plain) - 0.3) < 1e-9 || pi.stepCount > 0, "plain round trip");
        std::printf("    param %u \"%s\" %s %s steps=%d\n", pi.id, narrow(pi.title).c_str(), narrow(text).c_str(),
                    narrow(pi.units).c_str(), pi.stepCount);
        if (visible++ == 0) first = pi.id;
    }
    if (instrument) {
        IMidiMapping* mm = nullptr;
        CHECK(ctrl->queryInterface(IMidiMapping::iid, (void**)&mm) == kResultOk && mm, "IMidiMapping");
        if (mm) {
            ParamID id = 0;
            CHECK(mm->getMidiControllerAssignment(0, 0, kCtrlModWheel, id) == kResultOk, "mod wheel mapped");
            ParameterInfo pi{};
            bool found = false;
            for (int32 i = 0; i < np; i++) {
                ctrl->getParameterInfo(i, pi);
                if (pi.id == id) { found = (pi.flags & ParameterInfo::kIsHidden) != 0; break; }
            }
            CHECK(found, "mapped parameter exists and is hidden");
            CHECK(mm->getMidiControllerAssignment(0, 0, kPitchBend, id) == kResultOk, "pitch bend mapped");
            mm->release();
        }
    }

    // State: set a value through processing, save, restore elsewhere.
    ProcessSetup setup{kRealtime, kSample32, 512, 48000.0};
    CHECK(proc->setupProcessing(setup) == kResultOk, "setupProcessing");
    CHECK(comp->setActive(true) == kResultOk, "setActive");
    CHECK(proc->setProcessing(true) == kResultOk, "setProcessing");
    std::printf("    latency %u, tail %u\n", proc->getLatencySamples(), proc->getTailSamples());

    static float in[8][512], out[8][512];
    for (int c = 0; c < 8; c++)
        for (int i = 0; i < 512; i++) in[c][i] = 0.5f * std::sin(i * 0.05f + c);
    float* ip[8]; float* op[8];
    for (int c = 0; c < 8; c++) { ip[c] = in[c]; op[c] = out[c]; }
    AudioBusBuffers ib{}, ob{};
    ib.numChannels = in_ch; ib.channelBuffers32 = ip;
    ob.numChannels = out_ch; ob.channelBuffers32 = op;

    Changes changes;
    changes.queues.resize(instrument ? 2 : 1);
    changes.queues[0].id = first;
    changes.queues[0].points = {{0, 0.25}, {300, 0.75}};
    if (instrument) {
        ParamID modwheel = 0;
        IMidiMapping* mm = nullptr;
        ctrl->queryInterface(IMidiMapping::iid, (void**)&mm);
        mm->getMidiControllerAssignment(0, 0, kCtrlModWheel, modwheel);
        mm->release();
        changes.queues[1].id = modwheel;
        changes.queues[1].points = {{40, 1.0}};
    }
    Events events;
    if (instrument) {
        Event on{};
        on.type = Event::kNoteOnEvent; on.sampleOffset = 16; on.flags = Event::kIsLive;
        on.noteOn.channel = 0; on.noteOn.pitch = 60; on.noteOn.velocity = 0.9f; on.noteOn.noteId = 1;
        events.events.push_back(on);
        Event expr{};
        expr.type = Event::kNoteExpressionValueEvent; expr.sampleOffset = 64;
        expr.noteExpressionValue.typeId = kTuningTypeID; expr.noteExpressionValue.noteId = 1;
        expr.noteExpressionValue.value = 0.5;
        events.events.push_back(expr);
    }
    ProcessContext ctx{};
    ctx.state = ProcessContext::kPlaying | ProcessContext::kTempoValid | ProcessContext::kProjectTimeMusicValid;
    ctx.sampleRate = 48000.0; ctx.tempo = 120.0; ctx.projectTimeMusic = 4.0;
    ProcessData d{};
    d.processMode = kRealtime; d.symbolicSampleSize = kSample32; d.numSamples = 512;
    d.numInputs = in_ch ? 1 : 0; d.numOutputs = 1;
    d.inputs = in_ch ? &ib : nullptr; d.outputs = &ob;
    d.inputParameterChanges = &changes; d.inputEvents = &events; d.processContext = &ctx;
    std::memset(out, 0, sizeof out);
    CHECK(proc->process(d) == kResultOk, "process");
    double energy = 0; bool finite = true;
    for (int c = 0; c < out_ch; c++)
        for (int i = 0; i < 512; i++) { energy += out[c][i] * out[c][i]; finite &= std::isfinite(out[c][i]); }
    CHECK(finite, "finite output");
    CHECK(energy > 1e-4, "signal (energy %g)", energy);
    if (instrument)
        for (int i = 0; i < 16; i++) CHECK(out[0][i] == 0.0f, "silent before the note");
    CHECK(std::fabs(ctrl->getParamNormalized(first) - 0.75) < 1e-9, "automation reached the controller");

    // Flush call (no audio), odd block sizes, no context.
    ProcessData flush{};
    flush.symbolicSampleSize = kSample32; flush.numSamples = 0;
    CHECK(proc->process(flush) == kResultOk, "parameter flush");
    for (int32 n : {1, 3, 511, 64}) {
        Changes none; Events noev;
        d.numSamples = n; d.inputParameterChanges = &none; d.inputEvents = &noev; d.processContext = nullptr;
        CHECK(proc->process(d) == kResultOk, "block of %d", n);
    }

    Stream s;
    CHECK(comp->getState(&s) == kResultOk && !s.data.empty(), "getState");
    IComponent* comp2 = nullptr;
    f->createInstance(ci.cid, IComponent::iid, (void**)&comp2);
    comp2->initialize(host);
    IEditController* ctrl2 = nullptr;
    comp2->queryInterface(IEditController::iid, (void**)&ctrl2);
    s.pos = 0;
    CHECK(comp2->setState(&s) == kResultOk, "setState");
    s.pos = 0;
    CHECK(ctrl2->setComponentState(&s) == kResultOk, "setComponentState");
    CHECK(std::fabs(ctrl2->getParamNormalized(first) - 0.75) < 1e-9, "state restored (%g)", ctrl2->getParamNormalized(first));
    Stream junk; junk.data = {1, 2, 3};
    CHECK(comp2->setState(&junk) != kResultOk, "garbage state refused");
    ctrl2->release();
    comp2->terminate();
    comp2->release();

    CHECK(proc->setProcessing(false) == kResultOk, "stop");
    CHECK(comp->setActive(false) == kResultOk, "deactivate");
    // Reactivate at a new rate.
    ProcessSetup s2{kRealtime, kSample32, 128, 96000.0};
    CHECK(proc->setupProcessing(s2) == kResultOk && comp->setActive(true) == kResultOk, "reactivate");
    comp->setActive(false);
    ctrl->setComponentHandler(nullptr);
    CHECK(handler.refs == 1, "handler reference returned (%u)", handler.refs);
    ctrl->release();
    proc->release();
    CHECK(comp->terminate() == kResultOk, "terminate");
    comp->release();
}

int main(int argc, char** argv) {
    if (argc != 2) { std::fprintf(stderr, "usage: %s module\n", argv[0]); return 2; }
    void* lib = LOAD(argv[1]);
    if (!lib) { std::fprintf(stderr, "cannot load %s\n", argv[1]); return 1; }
#if defined(_WIN32)
    auto entry = (bool (*)(void*))nullptr;
    auto exit_ = (bool (*)())SYM(lib, "ExitDll");
    auto init = (bool (*)())SYM(lib, "InitDll");
    CHECK(init && exit_, "InitDll/ExitDll exported");
    if (init) CHECK(init(), "InitDll");
#elif defined(__APPLE__)
    auto entry = (bool (*)(void*))SYM(lib, "bundleEntry");
    auto exit_ = (bool (*)())SYM(lib, "bundleExit");
    CHECK(entry && exit_, "bundleEntry/bundleExit exported");
#else
    auto entry = (bool (*)(void*))SYM(lib, "ModuleEntry");
    auto exit_ = (bool (*)())SYM(lib, "ModuleExit");
    CHECK(entry && exit_, "ModuleEntry/ModuleExit exported");
#endif
    if (entry) CHECK(entry(lib), "ModuleEntry");
    auto get = (IPluginFactory * (PLUGIN_API*)())SYM(lib, "GetPluginFactory");
    CHECK(get != nullptr, "GetPluginFactory exported");
    if (!get) return 1;
    IPluginFactory* f = get();
    CHECK(f != nullptr, "factory");
    PFactoryInfo fi{};
    CHECK(f->getFactoryInfo(&fi) == kResultOk, "getFactoryInfo");
    std::printf("%s: vendor \"%s\", %d class(es)\n", argv[1], fi.vendor, f->countClasses());
    CHECK(fi.flags & PFactoryInfo::kUnicode, "unicode flag");
    IPluginFactory2* f2 = nullptr;
    IPluginFactory3* f3 = nullptr;
    CHECK(f->queryInterface(IPluginFactory2::iid, (void**)&f2) == kResultOk, "IPluginFactory2");
    CHECK(f->queryInterface(IPluginFactory3::iid, (void**)&f3) == kResultOk, "IPluginFactory3");
    HostApp host;
    if (f3) {
        CHECK(f3->setHostContext(&host) == kResultOk, "setHostContext");
        PClassInfoW w{};
        CHECK(f3->getClassInfoUnicode(0, &w) == kResultOk, "getClassInfoUnicode");
        PClassInfo2 a{};
        f2->getClassInfo2(0, &a);
        CHECK(narrow(w.name) == a.name, "unicode name matches");
        f3->release();
    }
    PClassInfo bad{};
    CHECK(f->getClassInfo(f->countClasses(), &bad) != kResultOk, "out-of-range class refused");
    TUID nope = {0};
    void* obj = (void*)1;
    CHECK(f->createInstance(nope, IComponent::iid, &obj) != kResultOk && !obj, "unknown class refused");
    for (int32 i = 0; i < f->countClasses(); i++) test_class(f, f2, i, &host);
    if (f2) f2->release();
    f->release();
    if (exit_) CHECK(exit_(), "ModuleExit");
    CLOSE(lib);
    if (failures) { std::fprintf(stderr, "%d check(s) failed\n", failures); return 1; }
    std::printf("all VST3 checks passed\n");
    return 0;
}
