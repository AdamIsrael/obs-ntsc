// Build the OBS filter properties panel by walking ntsc-rs's own
// `SettingDescriptor` tree, so we get all ~60 NtscEffect controls (labels,
// ranges, enum options, group structure) without transcribing them.
//
// The top of the panel hosts the three controls we own (preset dropdown,
// custom JSON path, intensity slider); below that, a "NTSC parameters" group
// contains everything from ntsc-rs in its native order.

use std::ffi::CString;

use ntsc_rs::settings::{SettingDescriptor, SettingKind, SettingsList, standard::NtscEffect};
use obs_wrapper::{
    obs_sys::{
        obs_combo_format_OBS_COMBO_FORMAT_INT, obs_combo_type_OBS_COMBO_TYPE_LIST,
        obs_group_type_OBS_GROUP_CHECKABLE, obs_group_type_OBS_GROUP_NORMAL,
        obs_path_type_OBS_PATH_FILE, obs_path_type_OBS_PATH_FILE_SAVE, obs_properties_add_bool,
        obs_properties_add_float_slider, obs_properties_add_group, obs_properties_add_int,
        obs_properties_add_int_slider, obs_properties_add_list, obs_properties_add_path,
        obs_properties_create, obs_properties_t, obs_property_list_add_int,
        obs_property_modified_t, obs_property_set_modified_callback, obs_property_t,
    },
    properties::Properties,
    wrapper::PtrWrapper,
};

use crate::presets::PresetId;

pub fn build_properties(
    settings_list: &SettingsList<NtscEffect>,
    on_preset: obs_property_modified_t,
    on_json: obs_property_modified_t,
    on_export: obs_property_modified_t,
) -> Properties {
    unsafe {
        let p = obs_properties_create();
        add_top_controls(p, on_preset, on_json, on_export);

        // NTSC parameter group, populated from ntsc-rs's own descriptor tree.
        let ntsc_group = obs_properties_create();
        for desc in settings_list.setting_descriptors.iter() {
            add_descriptor(ntsc_group, desc);
        }
        obs_properties_add_group(
            p,
            c"ntsc_settings".as_ptr(),
            c"NTSC parameters".as_ptr(),
            obs_group_type_OBS_GROUP_NORMAL,
            ntsc_group,
        );

        Properties::from_raw(p)
    }
}

unsafe fn add_top_controls(
    p: *mut obs_properties_t,
    on_preset: obs_property_modified_t,
    on_json: obs_property_modified_t,
    on_export: obs_property_modified_t,
) {
    unsafe {
        let preset_prop = obs_properties_add_list(
            p,
            c"preset".as_ptr(),
            c"Preset".as_ptr(),
            obs_combo_type_OBS_COMBO_TYPE_LIST,
            obs_combo_format_OBS_COMBO_FORMAT_INT,
        );
        obs_property_list_add_int(preset_prop, c"VHS".as_ptr(), PresetId::Vhs.to_i64());
        obs_property_list_add_int(preset_prop, c"Broadcast".as_ptr(), PresetId::Broadcast.to_i64());
        obs_property_list_add_int(preset_prop, c"Composite".as_ptr(), PresetId::Composite.to_i64());
        obs_property_list_add_int(preset_prop, c"Custom...".as_ptr(), PresetId::Custom.to_i64());
        obs_property_set_modified_callback(preset_prop, on_preset);

        let json_prop = obs_properties_add_path(
            p,
            c"custom_json".as_ptr(),
            c"Custom preset JSON".as_ptr(),
            obs_path_type_OBS_PATH_FILE,
            c"JSON (*.json)".as_ptr(),
            std::ptr::null(),
        );
        obs_property_set_modified_callback(json_prop, on_json);

        obs_properties_add_float_slider(
            p,
            c"intensity".as_ptr(),
            c"Intensity".as_ptr(),
            0.0,
            1.0,
            0.01,
        );

        // Save-current-settings export. FILE_SAVE opens a native save dialog;
        // the field clears itself after a successful write so the widget acts
        // as a one-click trigger rather than a remembered path.
        let export_prop = obs_properties_add_path(
            p,
            c"export_path".as_ptr(),
            c"Save preset as...".as_ptr(),
            obs_path_type_OBS_PATH_FILE_SAVE,
            c"JSON (*.json)".as_ptr(),
            std::ptr::null(),
        );
        obs_property_set_modified_callback(export_prop, on_export);
    }
}

unsafe fn add_descriptor(
    props: *mut obs_properties_t,
    desc: &SettingDescriptor<NtscEffect>,
) -> *mut obs_property_t {
    let name = CString::new(desc.id.name).unwrap();
    let label = CString::new(desc.label).unwrap();
    unsafe {
        match &desc.kind {
            SettingKind::Boolean => {
                obs_properties_add_bool(props, name.as_ptr(), label.as_ptr())
            }
            SettingKind::Enumeration { options } => {
                let p = obs_properties_add_list(
                    props,
                    name.as_ptr(),
                    label.as_ptr(),
                    obs_combo_type_OBS_COMBO_TYPE_LIST,
                    obs_combo_format_OBS_COMBO_FORMAT_INT,
                );
                for opt in options {
                    let opt_label = CString::new(opt.label).unwrap();
                    obs_property_list_add_int(p, opt_label.as_ptr(), opt.index as i64);
                }
                p
            }
            SettingKind::Percentage { .. } => obs_properties_add_float_slider(
                props,
                name.as_ptr(),
                label.as_ptr(),
                0.0,
                1.0,
                0.001,
            ),
            SettingKind::IntRange { range } => {
                let lo = *range.start();
                let hi = *range.end();
                // A slider with an i32::MIN..i32::MAX range (random_seed) is
                // useless. Fall back to a plain spinner above ~1M span.
                if (hi as i64 - lo as i64) > 1_000_000 {
                    obs_properties_add_int(props, name.as_ptr(), label.as_ptr(), lo, hi, 1)
                } else {
                    obs_properties_add_int_slider(props, name.as_ptr(), label.as_ptr(), lo, hi, 1)
                }
            }
            SettingKind::FloatRange { range, .. } => {
                let lo = *range.start() as f64;
                let hi = *range.end() as f64;
                let step = ((hi - lo) / 1000.0).max(0.0001);
                obs_properties_add_float_slider(props, name.as_ptr(), label.as_ptr(), lo, hi, step)
            }
            SettingKind::Group { children } => {
                let sub = obs_properties_create();
                for child in children {
                    add_descriptor(sub, child);
                }
                obs_properties_add_group(
                    props,
                    name.as_ptr(),
                    label.as_ptr(),
                    obs_group_type_OBS_GROUP_CHECKABLE,
                    sub,
                )
            }
        }
    }
}
