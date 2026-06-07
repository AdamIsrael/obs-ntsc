mod colormatrix;
mod filter;
mod presets;
mod yuv;

use obs_wrapper::{
    log::Logger,
    obs_register_module, obs_string,
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

        let source = load_context
            .create_source_builder::<NtscFilter>()
            .enable_get_name()
            .enable_get_properties()
            .enable_update()
            .enable_filter_video()
            .build();
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
