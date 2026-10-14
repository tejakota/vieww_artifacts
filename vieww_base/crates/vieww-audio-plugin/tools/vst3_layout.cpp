#include <cstdio>
#include <cstddef>
#include "pluginterfaces/base/ipluginbase.h"
#include "pluginterfaces/vst/ivstcomponent.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "pluginterfaces/vst/ivstevents.h"
#include "pluginterfaces/vst/ivstprocesscontext.h"
using namespace Steinberg; using namespace Steinberg::Vst;
#define P(T) printf(#T " %zu align %zu\n", sizeof(T), alignof(T))
#define O(T,f) printf("  " #T "." #f " %zu\n", offsetof(T,f))
int main(){
 P(PFactoryInfo); O(PFactoryInfo,url); O(PFactoryInfo,email); O(PFactoryInfo,flags);
 P(PClassInfo); O(PClassInfo,cardinality); O(PClassInfo,category); O(PClassInfo,name);
 P(PClassInfo2); O(PClassInfo2,classFlags); O(PClassInfo2,subCategories); O(PClassInfo2,vendor); O(PClassInfo2,version); O(PClassInfo2,sdkVersion);
 P(PClassInfoW);
 P(BusInfo); O(BusInfo,mediaType); O(BusInfo,direction); O(BusInfo,channelCount); O(BusInfo,name); O(BusInfo,busType); O(BusInfo,flags);
 P(RoutingInfo);
 P(ProcessSetup); O(ProcessSetup,maxSamplesPerBlock); O(ProcessSetup,sampleRate);
 P(AudioBusBuffers); O(AudioBusBuffers,silenceFlags); O(AudioBusBuffers,channelBuffers32);
 P(ProcessData); O(ProcessData,numSamples); O(ProcessData,inputs); O(ProcessData,outputs); O(ProcessData,inputParameterChanges); O(ProcessData,inputEvents); O(ProcessData,processContext);
 P(ParameterInfo); O(ParameterInfo,title); O(ParameterInfo,shortTitle); O(ParameterInfo,units); O(ParameterInfo,stepCount); O(ParameterInfo,defaultNormalizedValue); O(ParameterInfo,unitId); O(ParameterInfo,flags);
 P(Event); O(Event,sampleOffset); O(Event,ppqPosition); O(Event,flags); O(Event,type); O(Event,noteOn);
 P(NoteOnEvent); O(NoteOnEvent,pitch); O(NoteOnEvent,tuning); O(NoteOnEvent,velocity); O(NoteOnEvent,length); O(NoteOnEvent,noteId);
 P(NoteOffEvent); O(NoteOffEvent,velocity); O(NoteOffEvent,noteId); O(NoteOffEvent,tuning);
 P(DataEvent); P(PolyPressureEvent); P(NoteExpressionValueEvent); P(NoteExpressionTextEvent); P(ChordEvent); P(ScaleEvent); P(LegacyMIDICCOutEvent);
 P(ProcessContext); O(ProcessContext,sampleRate); O(ProcessContext,projectTimeSamples); O(ProcessContext,systemTime); O(ProcessContext,continousTimeSamples); O(ProcessContext,projectTimeMusic); O(ProcessContext,barPositionMusic); O(ProcessContext,cycleStartMusic); O(ProcessContext,cycleEndMusic); O(ProcessContext,tempo); O(ProcessContext,timeSigNumerator); O(ProcessContext,timeSigDenominator); O(ProcessContext,chord); O(ProcessContext,smpteOffsetSubframes); O(ProcessContext,frameRate); O(ProcessContext,samplesToNextClock);
 printf("kVstAudioEffectClass=%s\nkVstComponentControllerClass=%s\n", kVstAudioEffectClass, kVstComponentControllerClass);
 printf("kFx=%s kInstrument=%s kInstrumentSynth=%s\n", PlugType::kFx, PlugType::kInstrument, PlugType::kInstrumentSynth);
 printf("kVstVersionString=%s\n", kVstVersionString);
 printf("kStereo=%llu kMono=%llu\n", (unsigned long long)SpeakerArr::kStereo, (unsigned long long)SpeakerArr::kMono);
 printf("kNoteOnEvent=%d kNoteOffEvent=%d kDataEvent=%d kPolyPressure=%d kNoteExprValue=%d kLegacyMIDICCOut=%d\n", Event::kNoteOnEvent, Event::kNoteOffEvent, Event::kDataEvent, Event::kPolyPressureEvent, Event::kNoteExpressionValueEvent, Event::kLegacyMIDICCOutEvent);
 printf("PI flags canAutomate=%d isReadOnly=%d isWrapAround=%d isList=%d isHidden=%d isProgramChange=%d isBypass=%d\n", ParameterInfo::kCanAutomate, ParameterInfo::kIsReadOnly, ParameterInfo::kIsWrapAround, ParameterInfo::kIsList, ParameterInfo::kIsHidden, ParameterInfo::kIsProgramChange, ParameterInfo::kIsBypass);
 printf("ctx flags playing=%d cycle=%d rec=%d systemTime=%d projTimeMusic=%d tempo=%d bar=%d cycleV=%d timeSig=%d contTime=%d\n", ProcessContext::kPlaying, ProcessContext::kCycleActive, ProcessContext::kRecording, ProcessContext::kSystemTimeValid, ProcessContext::kProjectTimeMusicValid, ProcessContext::kTempoValid, ProcessContext::kBarPositionValid, ProcessContext::kCycleValid, ProcessContext::kTimeSigValid, ProcessContext::kContTimeValid);
 printf("kAudio=%d kEvent=%d kInput=%d kOutput=%d kMain=%d kAux=%d kDefaultActive=%d kSample32=%d kSample64=%d kRealtime=%d kOffline=%d\n", kAudio, kEvent, kInput, kOutput, kMain, kAux, BusInfo::kDefaultActive, kSample32, kSample64, kRealtime, kOffline);
 printf("kUnicode=%d kManyInstances=%d kDistributable=%d kSimpleModeSupported=%d\n", PFactoryInfo::kUnicode, PClassInfo::kManyInstances, kDistributable, kSimpleModeSupported);
 printf("kReloadComponent=%d kParamValuesChanged=%d kLatencyChanged=%d kParamTitlesChanged=%d\n", kReloadComponent, kParamValuesChanged, kLatencyChanged, kParamTitlesChanged);
 printf("ctxreq tempo=%d timesig=%d projTimeMusic=%d barPos=%d cycle=%d transport=%d\n", IProcessContextRequirements::kNeedTempo, IProcessContextRequirements::kNeedTimeSignature, IProcessContextRequirements::kNeedProjectTimeMusic, IProcessContextRequirements::kNeedBarPositionMusic, IProcessContextRequirements::kNeedCycleMusic, IProcessContextRequirements::kNeedTransportState);
}
