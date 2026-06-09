mod colormatrix;
mod filter;
mod presets;
mod properties;
mod settings_io;
mod yuv;

use obs_wrapper::{
    log::Logger,
    obs_register_module, obs_string,
    obs_sys::OBS_SOURCE_ASYNC_VIDEO,
    prelude::*,
    source::Sourceable,
};

use filter::NtscFilter;

struct ObsNtscModule {
    context: ModuleContext,
}

impl Module for ObsNtscModule {
    fn new(context: ModuleContext) -> Self {
        Self { context }
    }

    fn get_ctx(&self) -> &ModuleContext {
        &self.context
    }

    fn load(&mut self, load_context: &mut LoadContext) -> bool {
        // Best-effort logger init; ignore Err if a logger was somehow already set.
        let _ = Logger::new().with_promote_debug(true).init();

        let mut source = load_context
            .create_source_builder::<NtscFilter>()
            .enable_get_name()
            .enable_get_defaults()
            .enable_get_properties()
            .enable_update()
            .enable_filter_video()
            .build();
        // obs-wrapper's builder only sets OBS_SOURCE_VIDEO when video_render
        // is wired and never sets OBS_SOURCE_ASYNC. For an async video filter
        // (one that uses filter_video on raw frames), OBS expects
        // OBS_SOURCE_ASYNC_VIDEO. Set it directly on the underlying
        // obs_source_info before registering.
        source.as_mut().output_flags |= OBS_SOURCE_ASYNC_VIDEO;
        load_context.register_source(source);
        log::info!("obs-ntsc loaded; filter registered");
        true
    }

    fn name() -> ObsString {
        obs_string!("obs-ntsc")
    }

    fn description() -> ObsString {
        obs_string!("NTSC / VHS video artifact filter (ntsc-rs)")
    }

    fn author() -> ObsString {
        obs_string!("Adam Israel")
    }
}

obs_register_module!(ObsNtscModule);

// Sourceable trait needs to be in scope for the builder.
#[allow(dead_code)]
fn _sourceable_in_scope() -> obs_wrapper::source::SourceType {
    <NtscFilter as Sourceable>::get_type()
}
