//! A polyphonic subtractive synthesizer — vieww-audio's voices behind the
//! plugin interface — exported as CLAP and VST3.

use vieww_audio::midi::Message;
use vieww_audio::synth::voice::{Poly, SubtractiveVoice};
use vieww_audio_plugin::{
    Buffers, Event, Kind, ParamInfo, Parametric, Plugin, PluginInfo, Status, Taper, Transport,
};

/// The synth.
#[derive(Debug)]
pub struct Synth {
    poly: Poly<SubtractiveVoice>,
    cutoff: f64,
    resonance: f64,
    level: f64,
    glide: f64,
}

static INFO: PluginInfo = PluginInfo {
    id: "dev.vieww.synth",
    name: "vieww Synth",
    vendor: "vieww",
    url: "https://vieww.dev",
    version: "1.0.0",
    description: "Sixteen-voice subtractive synthesizer",
    kind: Kind::Instrument,
    inputs: 0,
    outputs: 2,
    note_input: true,
    vst3_class_id: *b"dev.vieww.synth\0",
    features: &["synthesizer", "stereo"],
};

fn voices(rate: f32) -> Poly<SubtractiveVoice> {
    Poly::new((0..16).map(|_| SubtractiveVoice::new(rate)).collect(), rate)
}

impl Synth {
    fn apply(&mut self) {
        #[allow(clippy::cast_possible_truncation, reason = "audio parameters")]
        let (c, r) = (self.cutoff as f32, self.resonance as f32);
        for v in self.poly.voices_mut() {
            v.cutoff = c;
            v.resonance = r;
        }
        #[allow(clippy::cast_possible_truncation, reason = "seconds")]
        let g = self.glide as f32;
        self.poly.glide = g;
    }
}

impl Parametric for Synth {
    fn param_info(&self) -> Vec<ParamInfo> {
        vec![
            ParamInfo::new(0, "Cutoff", 40.0, 18_000.0, 2_400.0)
                .taper(Taper::Log)
                .unit("Hz"),
            ParamInfo::new(1, "Resonance", 0.5, 8.0, 1.2),
            ParamInfo::new(2, "Level", 0.0, 1.0, 0.7),
            ParamInfo::new(3, "Glide", 0.0, 0.5, 0.0).unit("s"),
        ]
    }

    fn get_param(&self, id: u32) -> Option<f64> {
        Some(match id {
            0 => self.cutoff,
            1 => self.resonance,
            2 => self.level,
            3 => self.glide,
            _ => return None,
        })
    }

    fn set_param(&mut self, id: u32, v: f64) {
        match id {
            0 => self.cutoff = v,
            1 => self.resonance = v,
            2 => self.level = v,
            3 => self.glide = v,
            _ => return,
        }
        self.apply();
    }
}

impl Plugin for Synth {
    fn info() -> &'static PluginInfo {
        &INFO
    }

    fn create() -> Self {
        let mut s = Self {
            poly: voices(48_000.0),
            cutoff: 2_400.0,
            resonance: 1.2,
            level: 0.7,
            glide: 0.0,
        };
        s.apply();
        s
    }

    fn activate(&mut self, rate: f64, _: u32) -> bool {
        #[allow(clippy::cast_possible_truncation, reason = "sample rate")]
        let r = rate as f32;
        self.poly = voices(r);
        self.apply();
        true
    }

    fn reset(&mut self) {
        self.poly.all_notes_off(true);
    }

    fn process(&mut self, b: &mut Buffers<'_, '_, '_>, events: &[Event], _: &Transport) -> Status {
        #[allow(clippy::cast_possible_truncation, reason = "audio")]
        let level = self.level as f32;
        let mut e = 0;
        for f in 0..b.frames() {
            while e < events.len() && events[e].time() as usize <= f {
                match events[e] {
                    Event::NoteOn {
                        channel,
                        key,
                        velocity,
                        ..
                    } => {
                        self.poly.note_on(channel, key, velocity);
                    }
                    Event::NoteOff { channel, key, .. } | Event::Choke { channel, key, .. } => {
                        self.poly.note_off(channel, key);
                    }
                    Event::Midi { data, .. } => {
                        let channel = data[0] & 15;
                        let m = match data[0] & 0xF0 {
                            0x90 => Some(Message::NoteOn {
                                channel,
                                key: data[1],
                                velocity: data[2],
                            }),
                            0x80 => Some(Message::NoteOff {
                                channel,
                                key: data[1],
                                velocity: data[2],
                            }),
                            0xB0 => Some(Message::ControlChange {
                                channel,
                                controller: data[1],
                                value: data[2],
                            }),
                            0xE0 => Some(Message::PitchBend {
                                channel,
                                value: ((i16::from(data[2]) << 7) | i16::from(data[1])) - 8_192,
                            }),
                            _ => None,
                        };
                        if let Some(m) = m {
                            self.poly.midi(&m);
                        }
                    }
                    Event::NoteExpression { .. } => {}
                }
                e += 1;
            }
            let (l, r) = self.poly.next_stereo();
            if b.output_channels() >= 2 {
                b.output(0)[f] = l * level;
                b.output(1)[f] = r * level;
            }
        }
        if self.poly.active() == 0 {
            Status::Sleep
        } else {
            Status::Continue
        }
    }
}

vieww_audio_plugin::export_clap!(Synth);
vieww_audio_plugin::export_vst3!(Synth);
