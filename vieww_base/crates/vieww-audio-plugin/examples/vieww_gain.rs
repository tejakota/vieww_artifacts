//! A stereo gain, drive and pan effect, exported as CLAP and VST3.
//!
//! `cargo build --example vieww_gain` produces a shared library that is
//! both a `.clap` (rename to `vieww_gain.clap`) and a VST3 module (place in
//! `vieww_gain.vst3/Contents/<arch>-linux/vieww_gain.so`).

use vieww_audio_plugin::{
    Buffers, Event, Kind, ParamInfo, Parametric, Plugin, PluginInfo, Status, Transport,
};

/// The effect.
#[derive(Debug)]
pub struct Gain {
    gain_db: f64,
    drive: f64,
    pan: f64,
    bypass: f64,
    /// Smoothed linear gains, so automation never clicks.
    smooth: [f32; 2],
    coeff: f32,
}

static INFO: PluginInfo = PluginInfo {
    id: "dev.vieww.gain",
    name: "vieww Gain",
    vendor: "vieww",
    url: "https://vieww.dev",
    version: "1.0.0",
    description: "Gain, soft drive and balance",
    kind: Kind::Effect,
    inputs: 2,
    outputs: 2,
    note_input: false,
    vst3_class_id: *b"dev.vieww.gain\0\0",
    features: &["utility", "distortion", "stereo"],
};

impl Parametric for Gain {
    fn param_info(&self) -> Vec<ParamInfo> {
        vec![
            ParamInfo::new(0, "Gain", -48.0, 24.0, 0.0).unit("dB"),
            ParamInfo::new(1, "Drive", 0.0, 1.0, 0.0),
            ParamInfo::new(2, "Pan", -1.0, 1.0, 0.0),
            ParamInfo::toggle(3, "Bypass", false),
        ]
    }

    fn get_param(&self, id: u32) -> Option<f64> {
        Some(match id {
            0 => self.gain_db,
            1 => self.drive,
            2 => self.pan,
            3 => self.bypass,
            _ => return None,
        })
    }

    fn set_param(&mut self, id: u32, v: f64) {
        match id {
            0 => self.gain_db = v,
            1 => self.drive = v,
            2 => self.pan = v,
            3 => self.bypass = v,
            _ => {}
        }
    }
}

impl Plugin for Gain {
    fn info() -> &'static PluginInfo {
        &INFO
    }

    fn create() -> Self {
        Self {
            gain_db: 0.0,
            drive: 0.0,
            pan: 0.0,
            bypass: 0.0,
            smooth: [1.0; 2],
            coeff: 0.001,
        }
    }

    fn activate(&mut self, rate: f64, _: u32) -> bool {
        // A 5 ms smoothing time constant.
        #[allow(clippy::cast_possible_truncation, reason = "a coefficient")]
        let c = (1.0 - (-1.0 / (0.005 * rate)).exp()) as f32;
        self.coeff = c;
        true
    }

    fn process(&mut self, b: &mut Buffers<'_, '_, '_>, _: &[Event], _: &Transport) -> Status {
        if self.bypass >= 0.5 {
            b.pass_through();
            return Status::ContinueIfNotQuiet;
        }
        let g = 10f64.powf(self.gain_db / 20.0);
        let theta = (self.pan + 1.0) * std::f64::consts::FRAC_PI_4;
        #[allow(clippy::cast_possible_truncation, reason = "audio")]
        let target = [
            (g * theta.cos() * std::f64::consts::SQRT_2) as f32,
            (g * theta.sin() * std::f64::consts::SQRT_2) as f32,
        ];
        #[allow(clippy::cast_possible_truncation, reason = "audio")]
        let drive = (1.0 + self.drive * 9.0) as f32;
        let norm = drive.tanh();
        let channels = b.output_channels().min(2);
        for (ch, &goal) in target.iter().enumerate().take(channels) {
            let n = b.frames();
            for i in 0..n {
                let x = b.input(ch).get(i).copied().unwrap_or(0.0);
                self.smooth[ch] += (goal - self.smooth[ch]) * self.coeff;
                let y = if self.drive > 0.0 {
                    (x * drive).tanh() / norm
                } else {
                    x
                };
                b.output(ch)[i] = y * self.smooth[ch];
            }
        }
        Status::ContinueIfNotQuiet
    }
}

vieww_audio_plugin::export_clap!(Gain);
vieww_audio_plugin::export_vst3!(Gain);
