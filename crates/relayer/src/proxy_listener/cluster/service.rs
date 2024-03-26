use atm0s_sdn::{
    base::{
        Service, ServiceBuilder, ServiceCtx, ServiceInput, ServiceOutput, ServiceSharedInput,
        ServiceWorker,
    },
    features::{FeaturesControl, FeaturesEvent},
    services::visualization,
};

pub const SERVICE_ID: u8 = 100;
pub const SERVICE_NAME: &str = "relay";

type SC = visualization::Control;
type SE = visualization::Event;
type TC = ();
type TW = ();

struct RelayService;

impl Service<FeaturesControl, FeaturesEvent, SC, SE, TC, TW> for RelayService {
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }

    fn on_input(
        &mut self,
        _ctx: &ServiceCtx,
        _now: u64,
        _input: ServiceInput<FeaturesEvent, SC, TC>,
    ) {
    }

    fn on_shared_input<'a>(&mut self, _ctx: &ServiceCtx, _now: u64, _input: ServiceSharedInput) {}

    fn pop_output(&mut self, _ctx: &ServiceCtx) -> Option<ServiceOutput<FeaturesControl, SE, TW>> {
        None
    }
}

struct RelayServiceWorker;

impl ServiceWorker<FeaturesControl, FeaturesEvent, SE, TC, TW> for RelayServiceWorker {
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }
}

#[derive(Default)]
pub struct RelayServiceBuilder;

impl ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW> for RelayServiceBuilder {
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }

    fn create(&self) -> Box<dyn Service<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>> {
        Box::new(RelayService)
    }

    fn create_worker(&self) -> Box<dyn ServiceWorker<FeaturesControl, FeaturesEvent, SE, TC, TW>> {
        Box::new(RelayServiceWorker)
    }
}
