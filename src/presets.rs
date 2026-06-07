use ntsc_rs::settings::standard::{
    ChromaLowpass, FbmNoiseSettings, HeadSwitchingMidLineSettings, HeadSwitchingSettings,
    NtscEffect, RingingSettings, TrackingNoiseSettings, VHSEdgeWaveSettings, VHSSettings,
    VHSSharpenSettings, VHSTapeSpeed,
};
use ntsc_rs::settings::SettingsBlock;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PresetId {
    Vhs,
    Broadcast,
    Composite,
}

impl PresetId {
    pub fn from_i64(v: i64) -> Self {
        match v {
            0 => Self::Vhs,
            1 => Self::Broadcast,
            2 => Self::Composite,
            _ => Self::Vhs,
        }
    }

    pub fn to_i64(self) -> i64 {
        match self {
            Self::Vhs => 0,
            Self::Broadcast => 1,
            Self::Composite => 2,
        }
    }
}

#[inline]
fn enabled<T>(settings: T) -> SettingsBlock<T> {
    SettingsBlock {
        enabled: true,
        settings,
    }
}

#[inline]
fn disabled<T: Default>() -> SettingsBlock<T> {
    SettingsBlock {
        enabled: false,
        settings: T::default(),
    }
}

pub fn for_id(id: PresetId) -> NtscEffect {
    let mut e = NtscEffect::default();
    match id {
        PresetId::Vhs => vhs(&mut e),
        PresetId::Broadcast => broadcast(&mut e),
        PresetId::Composite => composite(&mut e),
    }
    e
}

// Worn-VCR look at intensity=1.0: visible tracking band along the bottom,
// horizontal edge wobble, head-switching torn band at the top, heavy chroma
// bleed, snow specks.
fn vhs(e: &mut NtscEffect) {
    e.chroma_lowpass_in = ChromaLowpass::Full;
    e.chroma_lowpass_out = ChromaLowpass::Full;
    e.composite_sharpening = 1.0;
    e.luma_smear = 0.6;

    e.composite_noise = enabled(FbmNoiseSettings {
        frequency: 0.5,
        intensity: 0.08,
        detail: 2,
    });
    e.luma_noise = enabled(FbmNoiseSettings {
        frequency: 0.5,
        intensity: 0.02,
        detail: 2,
    });
    e.chroma_noise = enabled(FbmNoiseSettings {
        frequency: 0.05,
        intensity: 0.18,
        detail: 3,
    });
    e.snow_intensity = 0.0015;
    e.snow_anisotropy = 0.5;
    e.chroma_phase_noise_intensity = 0.005;

    e.ringing = enabled(RingingSettings {
        frequency: 0.45,
        power: 4.0,
        intensity: 4.0,
    });

    e.head_switching = enabled(HeadSwitchingSettings {
        height: 16,
        offset: 5,
        horiz_shift: 80.0,
        mid_line: enabled(HeadSwitchingMidLineSettings {
            position: 0.95,
            jitter: 0.05,
        }),
    });

    e.tracking_noise = enabled(TrackingNoiseSettings {
        height: 24,
        wave_intensity: 20.0,
        snow_intensity: 0.04,
        snow_anisotropy: 0.3,
        noise_intensity: 0.35,
    });

    e.vhs_settings = enabled(VHSSettings {
        // LP tape speed = more degraded chroma than SP; matches "worn home VCR".
        tape_speed: VHSTapeSpeed::LP,
        chroma_loss: 0.0002,
        sharpen: enabled(VHSSharpenSettings {
            intensity: 0.8,
            frequency: 1.0,
        }),
        edge_wave: enabled(VHSEdgeWaveSettings {
            intensity: 1.5,
            speed: 4.0,
            frequency: 0.05,
            detail: 2,
        }),
    });
}

// Off-air analog UHF: clean signal with subtle chroma softness, faint
// luminance grain, occasional snow specks. No tape mechanics.
fn broadcast(e: &mut NtscEffect) {
    e.chroma_lowpass_in = ChromaLowpass::Light;
    e.chroma_lowpass_out = ChromaLowpass::Light;
    e.composite_sharpening = 0.8;
    e.luma_smear = 0.3;

    e.composite_noise = enabled(FbmNoiseSettings {
        frequency: 0.5,
        intensity: 0.04,
        detail: 2,
    });
    e.luma_noise = enabled(FbmNoiseSettings {
        frequency: 0.5,
        intensity: 0.015,
        detail: 1,
    });
    e.chroma_noise = enabled(FbmNoiseSettings {
        frequency: 0.05,
        intensity: 0.08,
        detail: 2,
    });
    e.snow_intensity = 0.0003;
    e.snow_anisotropy = 0.5;
    e.chroma_phase_noise_intensity = 0.001;

    e.ringing = enabled(RingingSettings {
        frequency: 0.45,
        power: 4.0,
        intensity: 2.0,
    });

    e.head_switching = disabled();
    e.tracking_noise = disabled();
    e.vhs_settings = disabled();
}

// Composite cable / RCA: strong chroma bleed and dot crawl, mid ringing,
// slight chroma offset, faint snow. No tape mechanics. The classic
// "console plugged into a CRT via yellow/white/red" look.
fn composite(e: &mut NtscEffect) {
    e.chroma_lowpass_in = ChromaLowpass::Full;
    e.chroma_lowpass_out = ChromaLowpass::Full;
    e.composite_sharpening = 1.2;
    e.luma_smear = 0.5;

    e.composite_noise = enabled(FbmNoiseSettings {
        frequency: 0.5,
        intensity: 0.06,
        detail: 2,
    });
    e.luma_noise = enabled(FbmNoiseSettings {
        frequency: 0.5,
        intensity: 0.015,
        detail: 1,
    });
    e.chroma_noise = enabled(FbmNoiseSettings {
        frequency: 0.05,
        intensity: 0.12,
        detail: 2,
    });
    e.snow_intensity = 0.0006;
    e.snow_anisotropy = 0.5;
    e.chroma_phase_noise_intensity = 0.003;
    e.chroma_phase_error = 0.05;
    e.chroma_delay_horizontal = 0.5;

    e.ringing = enabled(RingingSettings {
        frequency: 0.45,
        power: 4.0,
        intensity: 3.5,
    });

    e.head_switching = disabled();
    e.tracking_noise = disabled();
    e.vhs_settings = disabled();
}

pub fn apply_intensity(effect: &NtscEffect, intensity: f32) -> NtscEffect {
    let t = intensity.clamp(0.0, 1.0);
    let mut out = effect.clone();

    out.composite_sharpening *= t;
    out.luma_smear *= t;
    out.snow_intensity *= t;
    out.chroma_phase_noise_intensity *= t;
    out.chroma_phase_error *= t;
    out.chroma_delay_horizontal *= t;

    out.composite_noise.settings.intensity *= t;
    out.luma_noise.settings.intensity *= t;
    out.chroma_noise.settings.intensity *= t;

    out.ringing.settings.intensity *= t;
    if t < 0.05 {
        out.ringing.enabled = false;
    }

    out.head_switching.settings.horiz_shift *= t;
    if t < 0.05 {
        out.head_switching.enabled = false;
    }

    out.tracking_noise.settings.wave_intensity *= t;
    out.tracking_noise.settings.snow_intensity *= t;
    out.tracking_noise.settings.noise_intensity *= t;
    if t < 0.05 {
        out.tracking_noise.enabled = false;
    }

    out.vhs_settings.settings.chroma_loss *= t;
    out.vhs_settings.settings.sharpen.settings.intensity *= t;
    out.vhs_settings.settings.edge_wave.settings.intensity *= t;
    if t < 0.05 {
        out.vhs_settings.enabled = false;
    }

    out
}
