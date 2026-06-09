// Bridge between OBS's obs_data settings store and ntsc-rs's NtscEffect.
//
// - Read path walks ntsc-rs's SettingDescriptor tree and pulls each value with
//   the matching `obs_data_get_*` call. OBS's get-with-default behavior means
//   keys we haven't explicitly written still return whatever was registered
//   in `set_defaults`, so we don't need to call `obs_data_get_json_with_defaults`
//   (which obs-sys 0.2.1 doesn't expose).
// - Write/apply path uses ntsc-rs's own `to_json_string`, parses with
//   `obs_data_create_from_json`, and merges with `obs_data_apply`. This means
//   custom .json files exported from the standalone app round-trip through
//   our settings with no manual field-by-field translation.

use std::ffi::CString;
use std::sync::OnceLock;

use ntsc_rs::settings::{AnySetting, SettingKind, SettingsList, standard::NtscEffect};
use obs_wrapper::obs_sys::{
    obs_data_apply, obs_data_create_from_json, obs_data_get_bool, obs_data_get_double,
    obs_data_get_int, obs_data_release, obs_data_set_default_bool, obs_data_set_default_double,
    obs_data_set_default_int, obs_data_t,
};

pub fn settings_list() -> &'static SettingsList<NtscEffect> {
    static LIST: OnceLock<SettingsList<NtscEffect>> = OnceLock::new();
    LIST.get_or_init(SettingsList::<NtscEffect>::new)
}

/// Build a NtscEffect from current OBS settings. Walks every ntsc-rs
/// descriptor and reads the matching key (defaults included automatically).
pub fn read_effect(settings_ptr: *mut obs_data_t) -> NtscEffect {
    let mut effect = NtscEffect::default();
    let list = settings_list();
    for desc in list.all_descriptors() {
        let Ok(key) = CString::new(desc.id.name) else {
            continue;
        };
        let any = match &desc.kind {
            SettingKind::Boolean | SettingKind::Group { .. } => AnySetting::Bool(unsafe {
                obs_data_get_bool(settings_ptr, key.as_ptr())
            }),
            SettingKind::IntRange { .. } => AnySetting::Int(
                unsafe { obs_data_get_int(settings_ptr, key.as_ptr()) } as i32,
            ),
            SettingKind::Enumeration { .. } => AnySetting::Enum(
                unsafe { obs_data_get_int(settings_ptr, key.as_ptr()) } as u32,
            ),
            SettingKind::FloatRange { .. } | SettingKind::Percentage { .. } => {
                AnySetting::Float(unsafe { obs_data_get_double(settings_ptr, key.as_ptr()) } as f32)
            }
        };
        let _ = (desc.id.set)(&mut effect, any);
    }
    effect
}

/// Set OBS settings defaults for every ntsc-rs field, derived from
/// `NtscEffect::default()`. Called from `GetDefaultsSource::get_defaults`.
pub fn set_defaults(settings_ptr: *mut obs_data_t) {
    let default = NtscEffect::default();
    let list = settings_list();
    for desc in list.all_descriptors() {
        let Ok(key) = CString::new(desc.id.name) else {
            continue;
        };
        let any = (desc.id.get)(&default);
        unsafe {
            match any {
                AnySetting::Bool(b) => obs_data_set_default_bool(settings_ptr, key.as_ptr(), b),
                AnySetting::Int(i) => {
                    obs_data_set_default_int(settings_ptr, key.as_ptr(), i as i64)
                }
                AnySetting::Enum(e) => {
                    obs_data_set_default_int(settings_ptr, key.as_ptr(), e as i64)
                }
                AnySetting::Float(f) => {
                    obs_data_set_default_double(settings_ptr, key.as_ptr(), f as f64)
                }
            }
        }
    }
}

/// Merge a NtscEffect's values into OBS settings as explicit values (not
/// defaults). Used when picking a preset or loading a custom JSON file — the
/// resulting explicit keys then drive the UI sliders.
pub fn apply_effect_to_settings(effect: &NtscEffect, settings_ptr: *mut obs_data_t) {
    let json = match settings_list().to_json_string(effect) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("obs-ntsc: failed to serialize effect: {e:?}");
            return;
        }
    };
    let Ok(cjson) = CString::new(json) else { return };

    // Safety: settings_ptr is owned by OBS for the duration of the callback /
    // defaults call. obs_data_create_from_json returns a refcounted obs_data
    // that we release after merging.
    unsafe {
        let source = obs_data_create_from_json(cjson.as_ptr());
        if source.is_null() {
            return;
        }
        obs_data_apply(settings_ptr, source);
        obs_data_release(source);
    }
}

/// Serialize the effect via ntsc-rs's own writer and write it to disk.
/// The resulting file is interop-compatible with the standalone app's
/// preset loader (and our own `load_preset_from_path`).
pub fn save_effect_to_path(effect: &NtscEffect, path: &str) -> Result<(), String> {
    let json = settings_list()
        .to_json_string(effect)
        .map_err(|e| format!("serialize: {e:?}"))?;
    std::fs::write(path, json).map_err(|e| format!("write {path:?}: {e}"))
}

/// Parse a `.json` file in the ntsc-rs preset format. Logs a warning and
/// returns `None` on any failure (file read or parse).
pub fn load_preset_from_path(path: &str) -> Option<NtscEffect> {
    let json = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("obs-ntsc: failed to read preset file {path:?}: {e}");
            return None;
        }
    };
    // `from_json` (vs the generic one) also handles the legacy ntscqt format
    // out of the box, so .json files from the old Qt-era standalone work too.
    match settings_list().from_json(&json) {
        Ok(effect) => Some(effect),
        Err(e) => {
            log::warn!("obs-ntsc: failed to parse preset JSON at {path:?}: {e:?}");
            None
        }
    }
}

