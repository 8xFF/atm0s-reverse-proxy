use atm0s_sdn::{
    base::{
        Service, ServiceBuilder, ServiceCtx, ServiceInput, ServiceOutput, ServiceSharedInput,
        ServiceWorker, ServiceWorkerCtx, ServiceWorkerInput, ServiceWorkerOutput,
    },
    features::{FeaturesControl, FeaturesEvent},
    services::visualization,
};

use super::{NodeInfo, UserData};

pub const SERVICE_ID: u8 = 100;
pub const SERVICE_NAME: &str = "relay";

type SC = visualization::Control<NodeInfo>;
type SE = visualization::Event<NodeInfo>;
type TC = ();
type TW = ();

struct RelayService;

impl Service<UserData, FeaturesControl, FeaturesEvent, SC, SE, TC, TW> for RelayService {
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
        _input: ServiceInput<UserData, FeaturesEvent, SC, TC>,
    ) {
    }

    fn on_shared_input<'a>(&mut self, _ctx: &ServiceCtx, _now: u64, _input: ServiceSharedInput) {}

    fn pop_output2(
        &mut self,
        _now: u64,
    ) -> Option<ServiceOutput<UserData, FeaturesControl, SE, TW>> {
        None
    }
}

struct RelayServiceWorker;

impl ServiceWorker<UserData, FeaturesControl, FeaturesEvent, SC, SE, TC, TW>
    for RelayServiceWorker
{
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }

    fn on_tick(&mut self, _ctx: &atm0s_sdn::base::ServiceWorkerCtx, _now: u64, _tick_count: u64) {}

    fn on_input(
        &mut self,
        _ctx: &ServiceWorkerCtx,
        _now: u64,
        _input: ServiceWorkerInput<UserData, FeaturesEvent, SC, TW>,
    ) {
    }

    fn pop_output2(
        &mut self,
        _now: u64,
    ) -> Option<ServiceWorkerOutput<UserData, FeaturesControl, FeaturesEvent, SC, SE, TC>> {
        None
    }
}

#[derive(Default)]
pub struct RelayServiceBuilder;

impl ServiceBuilder<UserData, FeaturesControl, FeaturesEvent, SC, SE, TC, TW>
    for RelayServiceBuilder
{
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }

    fn create(&self) -> Box<dyn Service<UserData, FeaturesControl, FeaturesEvent, SC, SE, TC, TW>> {
        Box::new(RelayService)
    }

    fn create_worker(
        &self,
    ) -> Box<dyn ServiceWorker<UserData, FeaturesControl, FeaturesEvent, SC, SE, TC, TW>> {
        Box::new(RelayServiceWorker)
    }
}
