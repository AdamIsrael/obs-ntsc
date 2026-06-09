use std::os::raw::c_char;

use obs_wrapper::{
    data::DataObj,
    obs_string,
    obs_sys::{
        obs_data_get_int, obs_data_get_string, obs_data_set_int, obs_data_set_string, obs_data_t,
        obs_properties_get, obs_properties_t, obs_property_set_visible, obs_property_t,
        obs_source_frame,
    },
    prelude::*,
    properties::Properties,
    source::{
        CreatableSourceContext, FilterVideoSource, GetDefaultsSource, GetNameSource,
        GetPropertiesSource, SourceContext, SourceType, Sourceable, UpdateSource,
        video::{VideoDataContext, VideoFormat},
    },
    wrapper::PtrWrapper,
};

use ntsc_rs::{
    ctx,
    settings::standard::NtscEffect,
    yiq_fielding::{
        Bgrx, BlitInfo, DeinterlaceMode, PixelFormat, Rgbx, Xbgr, Xrgb, YiqOwned, YiqView,
    },
};

use crate::colormatrix::ColorMatrix;
use crate::presets::{self, PresetId};
use crate::properties as ui;
use crate::settings_io;
use crate::yuv;

const PROP_PRESET: &str = "preset";
const PROP_INTENSITY: &str = "intensity";

// Hidden sentinels: prevent re-overwriting user-tweaked sliders every time
// OBS reopens the properties dialog. The modified callback only re-applies
// preset values when the selection has actually changed since the last apply.
const SENTINEL_LAST_PRESET: &core::ffi::CStr = c"_last_applied_preset";
const SENTINEL_LAST_JSON: &core::ffi::CStr = c"_last_applied_json";

pub struct NtscFilter {
    base_effect: NtscEffect,
    intensity: f32,
    frame_num: u64,
    // Reused per frame to avoid per-frame allocations. Sized to width*height*4
    // packed RGBA; grown on demand when frame dimensions change.
    rgba_scratch: Vec<u8>,
}

impl NtscFilter {
    fn read_settings(&mut self, settings: &mut DataObj) {
        // base_effect is now assembled from the per-parameter sliders, which
        // are populated by the preset-changed callback (or directly by the
        // user). Intensity still applies as a global scale.
        self.base_effect = settings_io::read_effect(settings.as_ptr_mut());
        let intensity = settings.get::<f64>(PROP_INTENSITY).unwrap_or(1.0) as f32;
        self.intensity = intensity.clamp(0.0, 1.0);
    }
}

impl Sourceable for NtscFilter {
    fn get_id() -> ObsString {
        obs_string!("obs_ntsc_filter")
    }

    fn get_type() -> SourceType {
        SourceType::FILTER
    }

    fn create(create: &mut CreatableSourceContext<Self>, _source: SourceContext) -> Self {
        let mut filter = NtscFilter {
            base_effect: presets::for_id(PresetId::Vhs),
            intensity: 1.0,
            frame_num: 0,
            rgba_scratch: Vec::new(),
        };
        filter.read_settings(&mut create.settings);
        filter
    }
}

impl GetNameSource for NtscFilter {
    fn get_name() -> ObsString {
        obs_string!("NTSC / VHS Effect")
    }
}

impl GetDefaultsSource for NtscFilter {
    fn get_defaults(settings: &mut DataObj) {
        // Seed defaults for every ntsc-rs parameter, derived from
        // NtscEffect::default(). The current UI doesn't display them yet, but
        // they're persisted so callers reading via obs_data_get_* (including
        // future descriptor-driven code) see real numbers instead of zeros.
        settings_io::set_defaults(settings.as_ptr_mut());
        settings.set_default::<i64>(PROP_PRESET, PresetId::Vhs.to_i64());
        settings.set_default::<f64>(PROP_INTENSITY, 1.0);
    }
}

impl GetPropertiesSource for NtscFilter {
    fn get_properties(&mut self) -> Properties {
        ui::build_properties(
            settings_io::settings_list(),
            Some(on_preset_changed),
            Some(on_custom_json_changed),
            Some(on_export_path_changed),
        )
    }
}

unsafe fn current_preset(settings: *mut obs_data_t) -> PresetId {
    PresetId::from_i64(unsafe { obs_data_get_int(settings, c"preset".as_ptr()) })
}

unsafe fn current_json_path(settings: *mut obs_data_t) -> Option<String> {
    let ptr = unsafe { obs_data_get_string(settings, c"custom_json".as_ptr()) };
    if ptr.is_null() {
        return None;
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(ptr) };
    match cstr.to_str() {
        Ok(s) if !s.is_empty() => Some(s.to_owned()),
        _ => None,
    }
}

unsafe fn set_json_visibility(props: *mut obs_properties_t, visible: bool) {
    let p = unsafe { obs_properties_get(props, c"custom_json".as_ptr() as *const c_char) };
    if !p.is_null() {
        unsafe { obs_property_set_visible(p, visible) };
    }
}

/// C callback: preset dropdown changed. Writes the chosen preset's NtscEffect
/// values into settings only if the preset actually changed since the last
/// apply (so customized sliders survive scene reopen).
unsafe extern "C" fn on_preset_changed(
    props: *mut obs_properties_t,
    _property: *mut obs_property_t,
    settings: *mut obs_data_t,
) -> bool {
    let preset = unsafe { current_preset(settings) };
    let is_custom = preset == PresetId::Custom;
    unsafe { set_json_visibility(props, is_custom) };

    let last = unsafe { obs_data_get_int(settings, SENTINEL_LAST_PRESET.as_ptr()) };
    if last == preset.to_i64() {
        // Same selection as last apply — leave the user's slider values alone.
        return true;
    }

    let effect = if is_custom {
        unsafe { current_json_path(settings) }
            .and_then(|p| settings_io::load_preset_from_path(&p))
    } else {
        Some(presets::for_id(preset))
    };

    if let Some(effect) = effect {
        settings_io::apply_effect_to_settings(&effect, settings);
        unsafe { obs_data_set_int(settings, SENTINEL_LAST_PRESET.as_ptr(), preset.to_i64()) };
        if is_custom {
            if let Some(p) = unsafe { current_json_path(settings) } {
                if let Ok(cp) = std::ffi::CString::new(p) {
                    unsafe {
                        obs_data_set_string(settings, SENTINEL_LAST_JSON.as_ptr(), cp.as_ptr())
                    };
                }
            }
        }
    }
    true
}

/// C callback: user picked a save path for the current settings. Reads the
/// current effect from settings, writes it to the picked path, then clears
/// the field so the widget behaves as a one-click "save now" trigger.
unsafe extern "C" fn on_export_path_changed(
    _props: *mut obs_properties_t,
    _property: *mut obs_property_t,
    settings: *mut obs_data_t,
) -> bool {
    let path_ptr = unsafe { obs_data_get_string(settings, c"export_path".as_ptr()) };
    if path_ptr.is_null() {
        return false;
    }
    let path = match unsafe { std::ffi::CStr::from_ptr(path_ptr) }.to_str() {
        Ok(s) if !s.is_empty() => s.to_owned(),
        _ => return false,
    };

    let effect = settings_io::read_effect(settings);
    match settings_io::save_effect_to_path(&effect, &path) {
        Ok(()) => log::info!("obs-ntsc: saved preset to {path:?}"),
        Err(e) => log::warn!("obs-ntsc: failed to save preset: {e}"),
    }

    // Clear the path so picking the same file again still triggers a save.
    unsafe { obs_data_set_string(settings, c"export_path".as_ptr(), c"".as_ptr()) };
    true
}

/// C callback: custom JSON path changed. Reloads + writes into settings only
/// when the path differs from the last applied one (same survival rule as
/// the preset callback).
unsafe extern "C" fn on_custom_json_changed(
    _props: *mut obs_properties_t,
    _property: *mut obs_property_t,
    settings: *mut obs_data_t,
) -> bool {
    if unsafe { current_preset(settings) } != PresetId::Custom {
        return false;
    }
    let Some(path) = (unsafe { current_json_path(settings) }) else {
        return false;
    };

    // Compare to last applied path.
    let last_ptr = unsafe { obs_data_get_string(settings, SENTINEL_LAST_JSON.as_ptr()) };
    if !last_ptr.is_null() {
        if let Ok(last) = unsafe { std::ffi::CStr::from_ptr(last_ptr) }.to_str() {
            if last == path {
                return false;
            }
        }
    }

    let Some(effect) = settings_io::load_preset_from_path(&path) else {
        return false;
    };
    settings_io::apply_effect_to_settings(&effect, settings);
    if let Ok(cp) = std::ffi::CString::new(path) {
        unsafe { obs_data_set_string(settings, SENTINEL_LAST_JSON.as_ptr(), cp.as_ptr()) };
    }
    true
}

impl UpdateSource for NtscFilter {
    fn update(&mut self, settings: &mut DataObj, _context: &mut GlobalContext) {
        self.read_settings(settings);
    }
}

impl NtscFilter {
    fn ensure_scratch(&mut self, width: usize, height: usize) {
        let needed = width.checked_mul(height).and_then(|p| p.checked_mul(4)).unwrap_or(0);
        if needed == 0 {
            return;
        }
        if self.rgba_scratch.len() != needed {
            self.rgba_scratch.resize(needed, 0);
        }
    }

    fn process_uyvy(
        &mut self,
        buf_ptr: *mut u8,
        linesize: usize,
        width: usize,
        height: usize,
        effect: &NtscEffect,
        frame_num: usize,
        cm: &ColorMatrix,
    ) {
        self.ensure_scratch(width, height);
        if self.rgba_scratch.is_empty() {
            return;
        }
        // Safety: linesize * height bytes valid for the callback (OBS owns it).
        let yuv = unsafe { std::slice::from_raw_parts_mut(buf_ptr, linesize * height) };
        yuv::uyvy_to_rgba(yuv, linesize, &mut self.rgba_scratch, width, height, cm);
        process_in_place::<Rgbx>(
            &mut self.rgba_scratch,
            width * 4,
            width,
            height,
            effect,
            frame_num,
        );
        yuv::rgba_to_uyvy(&self.rgba_scratch, yuv, linesize, width, height, cm);
    }

    fn process_nv12(
        &mut self,
        y_ptr: *mut u8,
        y_linesize: usize,
        uv_ptr: *mut u8,
        uv_linesize: usize,
        width: usize,
        height: usize,
        effect: &NtscEffect,
        frame_num: usize,
        cm: &ColorMatrix,
    ) {
        self.ensure_scratch(width, height);
        if self.rgba_scratch.is_empty() {
            return;
        }
        // Safety: OBS guarantees these planes are valid for the callback duration.
        let y_plane = unsafe { std::slice::from_raw_parts_mut(y_ptr, y_linesize * height) };
        let uv_plane =
            unsafe { std::slice::from_raw_parts_mut(uv_ptr, uv_linesize * (height / 2)) };
        yuv::nv12_to_rgba(
            y_plane,
            y_linesize,
            uv_plane,
            uv_linesize,
            &mut self.rgba_scratch,
            width,
            height,
            cm,
        );
        process_in_place::<Rgbx>(
            &mut self.rgba_scratch,
            width * 4,
            width,
            height,
            effect,
            frame_num,
        );
        yuv::rgba_to_nv12(
            &self.rgba_scratch,
            y_plane,
            y_linesize,
            uv_plane,
            uv_linesize,
            width,
            height,
            cm,
        );
    }
}

/// Reach into the raw frame to read color-matrix metadata that `obs-wrapper`'s
/// `VideoDataContext` doesn't expose.
///
/// Safety: relies on `VideoDataContext`'s layout being a single-field tuple
/// over `*mut obs_source_frame`. Verified against obs-wrapper 0.4.1; if that
/// crate ever adds another field, this transmute breaks and we need to
/// regenerate bindings ourselves.
unsafe fn color_matrix_from_video(video: &VideoDataContext) -> ColorMatrix {
    let frame_ptr: *mut obs_source_frame = unsafe {
        *(video as *const VideoDataContext as *const *mut obs_source_frame)
    };
    let frame = unsafe { &*frame_ptr };
    ColorMatrix::from_obs(
        &frame.color_matrix,
        &frame.color_range_min,
        &frame.color_range_max,
        frame.full_range,
    )
}

impl FilterVideoSource for NtscFilter {
    fn filter_video(&mut self, video: &mut VideoDataContext) {
        let format = video.get_format();
        let width = video.get_width() as usize;
        let height = video.get_height() as usize;

        if width == 0 || height == 0 {
            return;
        }

        // At zero intensity, pass through untouched — skipping ntsc-rs *and*
        // the YUV↔RGB roundtrip. Also keeps the slider feeling "off" at 0.
        if self.intensity < 0.01 {
            return;
        }

        let effect = presets::apply_intensity(&self.base_effect, self.intensity);
        let frame_num = self.frame_num as usize;
        self.frame_num = self.frame_num.wrapping_add(1);

        let linesize0 = video.get_linesize(0) as usize;
        let buf0_ptr = video.get_data_buffer(0);
        if buf0_ptr.is_null() || linesize0 == 0 {
            return;
        }

        match format {
            VideoFormat::RGBA => {
                let buf = unsafe { std::slice::from_raw_parts_mut(buf0_ptr, linesize0 * height) };
                process_in_place::<Rgbx>(buf, linesize0, width, height, &effect, frame_num);
            }
            VideoFormat::BGRA | VideoFormat::BGRX => {
                let buf = unsafe { std::slice::from_raw_parts_mut(buf0_ptr, linesize0 * height) };
                process_in_place::<Bgrx>(buf, linesize0, width, height, &effect, frame_num);
            }
            VideoFormat::UYVY => {
                let cm = unsafe { color_matrix_from_video(video) };
                self.process_uyvy(buf0_ptr, linesize0, width, height, &effect, frame_num, &cm);
            }
            VideoFormat::NV12 => {
                let linesize1 = video.get_linesize(1) as usize;
                let buf1_ptr = video.get_data_buffer(1);
                if buf1_ptr.is_null() || linesize1 == 0 {
                    return;
                }
                let cm = unsafe { color_matrix_from_video(video) };
                self.process_nv12(
                    buf0_ptr, linesize0, buf1_ptr, linesize1, width, height, &effect, frame_num, &cm,
                );
            }
            other => {
                if frame_num == 1 {
                    log::warn!(
                        "obs-ntsc: unsupported pixel format {:?} — passing frames through",
                        other
                    );
                }
            }
        }
    }
}

fn process_in_place<S: PixelFormat>(
    buf: &mut [u8],
    row_bytes: usize,
    width: usize,
    height: usize,
    effect: &NtscEffect,
    frame_num: usize,
) {
    let field = effect.use_field.to_yiq_field(frame_num);
    let mut yiq =
        YiqOwned::from_strided_buffer::<S, u8>(ctx::global(), &*buf, row_bytes, width, height, field);
    let mut view = YiqView::from(&mut yiq);
    effect.apply_effect_to_yiq(ctx::global(), &mut view, frame_num, [1.0, 1.0]);
    view.write_to_strided_buffer::<S, u8, _>(
        ctx::global(),
        buf,
        BlitInfo::from_full_frame(width, height, row_bytes),
        DeinterlaceMode::Bob,
        (),
    );
    // Silence unused-import warnings on platforms where these variants aren't reached.
    let _ = (std::marker::PhantomData::<Xrgb>, std::marker::PhantomData::<Xbgr>);
}
