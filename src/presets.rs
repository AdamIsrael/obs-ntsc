use ntsc_rs::settings::standard::{
    ChromaLowpass, HeadSwitchingSettings, NtscEffect, RingingSettings, VHSSettings, VHSTapeSpeed,
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

pub fn for_id(id: PresetId) -> NtscEffect {
    let mut e = NtscEffect::default();
    match id {
        PresetId::Vhs => {
            e.chroma_lowpass_in = ChromaLowpass::Full;
            e.chroma_lowpass_out = ChromaLowpass::Full;
            e.composite_sharpening = 1.2;
            e.snow_intensity = 0.0008;
            e.ringing = SettingsBlock {
                enabled: true,
                settings: RingingSettings {
                    frequency: 0.45,
                    power: 4.0,
                    intensity: 5.0,
                },
            };
            e.head_switching = SettingsBlock {
                enabled: true,
                settings: HeadSwitchingSettings {
                    height: 8,
                    offset: 3,
                    horiz_shift: 72.0,
                    mid_line: Default::default(),
                },
            };
            e.vhs_settings = SettingsBlock {
                enabled: true,
                settings: VHSSettings {
                    tape_speed: VHSTapeSpeed::SP,
                    chroma_loss: 0.00005,
                    sharpen: Default::default(),
                    edge_wave: Default::default(),
                },
            };
        }
        PresetId::Broadcast => {
            e.chroma_lowpass_in = ChromaLowpass::Light;
            e.chroma_lowpass_out = ChromaLowpass::Light;
            e.composite_sharpening = 0.5;
            e.snow_intensity = 0.00005;
            e.ringing = SettingsBlock {
                enabled: true,
                settings: RingingSettings {
                    frequency: 0.45,
                    power: 4.0,
                    intensity: 1.5,
                },
            };
            e.head_switching = SettingsBlock {
                enabled: false,
                settings: HeadSwitchingSettings::default(),
            };
            e.vhs_settings = SettingsBlock {
                enabled: false,
                settings: VHSSettings::default(),
            };
        }
        PresetId::Composite => {
            e.chroma_lowpass_in = ChromaLowpass::Full;
            e.chroma_lowpass_out = ChromaLowpass::Full;
            e.composite_sharpening = 0.9;
            e.snow_intensity = 0.0002;
            e.ringing = SettingsBlock {
                enabled: true,
                settings: RingingSettings {
                    frequency: 0.45,
                    power: 4.0,
                    intensity: 3.0,
                },
            };
            e.head_switching = SettingsBlock {
                enabled: false,
                settings: HeadSwitchingSettings::default(),
            };
            e.vhs_settings = SettingsBlock {
                enabled: false,
                settings: VHSSettings::default(),
            };
        }
    }
    e
}

pub fn apply_intensity(effect: &NtscEffect, intensity: f32) -> NtscEffect {
    let t = intensity.clamp(0.0, 1.0);
    let mut out = effect.clone();

    out.composite_sharpening *= t;
    out.luma_smear *= t;
    out.snow_intensity *= t;
    out.chroma_phase_noise_intensity *= t;

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

    out.vhs_settings.settings.chroma_loss *= t;
    out.vhs_settings.settings.sharpen.settings.intensity *= t;
    out.vhs_settings.settings.edge_wave.settings.intensity *= t;
    if t < 0.05 {
        out.vhs_settings.enabled = false;
    }

    out
}
