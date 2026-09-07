use std::{convert::Infallible, sync::Arc, time::Duration};

use overwatch::{
    DynError, OpaqueServiceResourcesHandle,
    overwatch::{OverwatchHandle, OverwatchRunner},
    services::{
        ServiceCore, ServiceData,
        state::{NoOperator, NoState, StateOperator},
    },
};
use overwatch_derive::derive_services;
use tokio::{
    runtime::Handle,
    sync::{Notify, oneshot},
    time::timeout,
};

#[derive(Clone, Debug)]
struct RecoverySettings {
    entered: Arc<Notify>,
    stopping: Arc<Notify>,
    acquired: Arc<Notify>,
    check_shutdown_rejections: bool,
}

#[derive(Clone)]
struct RecoveryOperator {
    settings: RecoverySettings,
    handle: OverwatchHandle<RuntimeServiceId>,
}

#[async_trait::async_trait]
impl StateOperator<RuntimeServiceId> for RecoveryOperator {
    type State = NoState<RecoverySettings>;
    type LoadError = Infallible;

    fn try_load(_settings: &RecoverySettings) -> Result<Option<Self::State>, Self::LoadError> {
        Ok(None)
    }

    fn from_settings(
        settings: &RecoverySettings,
        handle: OverwatchHandle<RuntimeServiceId>,
    ) -> Self {
        Self {
            settings: settings.clone(),
            handle,
        }
    }

    async fn run(&mut self, _state: Self::State) {
        self.settings.entered.notify_one();
        // The other service only releases this gate from Drop, after the
        // manager has entered shutdown and dispatched its stop messages.
        self.settings.stopping.notified().await;
        if self.settings.check_shutdown_rejections {
            assert!(self.handle.shutdown().await.is_err());
            assert!(
                self.handle
                    .start_service::<RecoveringService>()
                    .await
                    .is_err()
            );
        }
        assert_eq!(self.handle.retrieve_service_ids().await.unwrap().len(), 2);
        let _watcher = self.handle.status_watcher::<RecoveringService>().await;
        let relay = self.handle.relay::<RecoveringService>().await.unwrap();
        let (sender, receiver) = oneshot::channel();
        relay.send(sender).await.unwrap();
        receiver.await.unwrap();
        self.settings.acquired.notify_one();
    }
}

struct RecoveringService {
    resources: OpaqueServiceResourcesHandle<Self, RuntimeServiceId>,
}

impl ServiceData for RecoveringService {
    type Settings = RecoverySettings;
    type State = NoState<Self::Settings>;
    type StateOperator = RecoveryOperator;
    type Message = oneshot::Sender<()>;
}

#[async_trait::async_trait]
impl ServiceCore<RuntimeServiceId> for RecoveringService {
    fn init(
        resources: OpaqueServiceResourcesHandle<Self, RuntimeServiceId>,
        _state: Self::State,
    ) -> Result<Self, DynError> {
        Ok(Self { resources })
    }

    async fn run(mut self) -> Result<(), DynError> {
        while let Some(sender) = self.resources.inbound_relay.recv().await {
            sender
                .send(())
                .map_err(|()| "recovery reply receiver was dropped")?;
        }
        Ok(())
    }
}

struct ShutdownWitness {
    stopping: Arc<Notify>,
}

impl ServiceData for ShutdownWitness {
    type Settings = Arc<Notify>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = ();
}

#[async_trait::async_trait]
impl ServiceCore<RuntimeServiceId> for ShutdownWitness {
    fn init(
        resources: OpaqueServiceResourcesHandle<Self, RuntimeServiceId>,
        _state: Self::State,
    ) -> Result<Self, DynError> {
        Ok(Self {
            stopping: resources.settings_handle.notifier().get_updated_settings(),
        })
    }

    async fn run(self) -> Result<(), DynError> {
        std::future::pending().await
    }
}

impl Drop for ShutdownWitness {
    fn drop(&mut self) {
        self.stopping.notify_one();
    }
}

#[derive_services]
struct App {
    recovering: RecoveringService,
    witness: ShutdownWitness,
}

async fn check_shutdown_recovery(check_shutdown_rejections: bool) {
    let settings = RecoverySettings {
        entered: Arc::new(Notify::new()),
        stopping: Arc::new(Notify::new()),
        acquired: Arc::new(Notify::new()),
        check_shutdown_rejections,
    };
    let app = OverwatchRunner::<App>::run(
        AppServiceSettings {
            recovering: settings.clone(),
            witness: Arc::clone(&settings.stopping),
        },
        Some(Handle::current()),
    )
    .unwrap();
    let handle = app.handle();
    handle.start_all_services().await.unwrap();
    timeout(Duration::from_secs(1), settings.entered.notified())
        .await
        .unwrap();
    timeout(Duration::from_secs(1), handle.shutdown())
        .await
        .expect("shutdown must continue serving the relay needed by the state operator")
        .unwrap();
    timeout(Duration::from_secs(1), settings.acquired.notified())
        .await
        .unwrap();
    app.wait_finished().await;
}

#[tokio::test]
async fn shutdown_services_can_finish_recovery_on_current_thread() {
    check_shutdown_recovery(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_services_can_finish_recovery_on_multiple_threads() {
    check_shutdown_recovery(false).await;
}

#[tokio::test]
async fn shutdown_rejects_restart_and_duplicate_shutdown_without_blocking_recovery() {
    check_shutdown_recovery(true).await;
}
