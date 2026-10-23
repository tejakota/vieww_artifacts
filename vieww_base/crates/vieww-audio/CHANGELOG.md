# Changelog — vieww-audio

All notable changes to this crate. Dates are when the change was made;
the crate is unpublished, so there are no version tags yet.

## Unreleased — 2026-10-23

### Fixed
- `Mixer::render`: renders the full length of every tone, at the right
  gain, without clipping overlaps or dropping the final frame.
- `Samples`: fallible `try_*` constructors for odd-length stereo data and
  zero rates, where the panicking ones would abort.
- A NaN from one insert effect no longer silences its mixer channel
  forever: the chain is reset and the fault counted.
- Note-on paths allocated (polyphony voice picking, Karplus-Strong pluck,
  modal strike, sampler note-on, sustain pedal); they no longer do.
- Plugin hosts (CLAP, VST3) sanitise non-finite plugin output.

### Added — real-time and devices
- `rt`: real-time priority for device threads (SCHED_FIFO with reset on
  fork, MMCSS "Pro Audio", Mach time-constraint), helper-thread elevation,
  `VIEWW_AUDIO_NO_RT` opt-out.
- Stream telemetry: scheduling class, render time mean and max, peak
  load, late renders, output latency (every backend reports latency).
- Device lists and selection: `Backend::devices`, `open_output_on`,
  `default_device_id` for ALSA, PulseAudio, PipeWire, JACK, WASAPI and
  Core Audio; `backend::devices()`.
- `backend::resilient::ResilientStream`: survives device loss and
  default-device changes with the same render closure; returns to a named
  device when it comes back (including after a sound server moved the
  stream elsewhere).
- ALSA: negotiates the device's best format (F32, S32, S24, S24_3LE, S16)
  and the requested rate or the nearest standard one, resampling to it;
  dithered 16- and 24-bit output. `device::PcmFormat`, `negotiate`,
  `encode_samples`, `decode_samples`.
- `backend::wasapi::WasapiExclusive`: exclusive-mode streams at the
  endpoint's native format and a one-period buffer.
- Web: worker mode (render in a Web Worker, page only hosts the context)
  and a shared-memory ring transport with an allocation-free worklet;
  messages remain the fallback.

### Added — music and formats
- `synth::tuning`: equal temperaments of any division, scale tunings,
  tuning tables; voices and the sampler play in any tuning, with
  fractional pitch and per-channel bend. `mts`: MIDI Tuning Standard.
- Instruments: tambura (jawari), bendable Karplus-Strong, mridangam and
  drum kit (`synth::percussion`).
- `codec::tags`: tag metadata read from MP3 (ID3v2.2–2.4, ID3v1), FLAC,
  Ogg Vorbis and Opus, WAV, AIFF and MP4; written to WAV, FLAC and AIFF.
- `midi_io`: system MIDI ports — `input_ports`, `output_ports`,
  `MidiInput::open`, `MidiOutput::open` — on Linux (rawmidi), Windows
  (WinMM, SysEx both ways) and macOS/iOS (CoreMIDI).

### Verification added
- Live backend tests under full-core load and a PulseAudio unplug/replug
  test (`tests/live_backends.rs`, ignored by default; need servers).
- No-allocation proof extended to polyphony, every stereo effect, NaN
  recovery, the sampler, struck instruments and rate-matched output.
- Tag readers against ffmpeg-tagged fixtures; tag writers read back by
  ffprobe.
- Web transports driven under Node with a simulated Web Audio.

### Not verified here
Windows (WASAPI exclusive, WinMM) and Apple (Core Audio device
selection, CoreMIDI) code is compile-checked with clippy for those
targets and has layout tests, but has not run on those systems. ALSA
format negotiation is unit-tested; no ALSA hardware was available.
