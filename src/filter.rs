use std::ops::RangeInclusive;

use obs_wrapper::{
    data::DataObj,
    obs_string,
    obs_sys::obs_source_frame,
    prelude::*,
    properties::{NumberProp, Properties},
    source::{
        CreatableSourceContext, FilterVideoSource, GetNameSource, GetPropertiesSource,
        SourceContext, SourceType, Sourceable, UpdateSource,
        video::{VideoDataContext, VideoFormat},
    },
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
use crate::yuv;

// Property keys — duplicated as literals into obs_string! and as &str for DataObj::get<T>().
// obs_string! requires literals, so keeping these in lockstep is intentional.
const PROP_PRESET: &str = "preset";
const PROP_INTENSITY: &str = "intensity";

pub struct NtscFilter {
    base_effect: NtscEffect,
    intensity: f32,
    frame_num: u64,
    // Reused per frame to avoid per-frame allocations. Sized to width*height*4
    // packed RGBA; grown on demand when frame dimensions change.
    rgba_scratch: Vec<u8>,
}

impl NtscFilter {
    fn read_settings(&mut self, settings: &DataObj) {
        let preset_raw = settings.get::<i64>(PROP_PRESET).unwrap_or(0);
        let intensity = settings.get::<f64>(PROP_INTENSITY).unwrap_or(1.0) as f32;

        self.base_effect = presets::for_id(PresetId::from_i64(preset_raw));
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
        filter.read_settings(&create.settings);
        filter
    }
}

impl GetNameSource for NtscFilter {
    fn get_name() -> ObsString {
        obs_string!("NTSC / VHS Effect")
    }
}

impl GetPropertiesSource for NtscFilter {
    fn get_properties(&mut self) -> Properties {
        let mut props = Properties::new();

        let mut preset_list = props.add_list::<i64>(
            obs_string!("preset"),
            obs_string!("Preset"),
            false,
        );
        preset_list.push(obs_string!("VHS"), PresetId::Vhs.to_i64());
        preset_list.push(obs_string!("Broadcast"), PresetId::Broadcast.to_i64());
        preset_list.push(obs_string!("Composite"), PresetId::Composite.to_i64());
        drop(preset_list);

        let intensity: RangeInclusive<f64> = 0.0..=1.0;
        props.add(
            obs_string!("intensity"),
            obs_string!("Intensity"),
            NumberProp::new_float(0.01_f64)
                .with_range(intensity)
                .with_slider(),
        );

        props
    }
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
