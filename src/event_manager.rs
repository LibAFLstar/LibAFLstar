//! Literal copy of SimpleEventManger from LibAFL with a few tweaks, namely that we misuse the functionality to have multiple fuzzing clients (processes)
//! show stats, but instead we use it for different states.
use std::fmt::Debug;
use std::marker::PhantomData;

use libafl::{
    events::{
        BrokerEventResult, CustomBufEventResult, Event, EventFirer, EventManager, EventManagerId,
        EventProcessor, EventRestarter, HasCustomBufHandlers, HasEventManagerId, ProgressReporter,
    },
    inputs::UsesInput,
    monitors::Monitor,
    state::{HasExecutions, HasLastReportTime, HasMetadata, State, UsesState},
};
use libafl_bolts::{ClientId, Error};

use crate::state::MultipleStates;

type CustomBufHandlerFn<S> = dyn FnMut(&mut S, &str, &[u8]) -> Result<CustomBufEventResult, Error>;

/// A simple, single-threaded event manager that just logs
/// but sets the ClientId to be the current state as given by the [`MultipleStates::current_state_idx`] function.
pub struct LibAFLStarManager<MT, S>
where
    S: UsesInput,
{
    /// The monitor
    monitor: MT,
    /// The events that happened since the last handle_in_broker
    events: Vec<Event<S::Input>>,
    /// The custom buf handler
    custom_buf_handlers: Vec<Box<CustomBufHandlerFn<S>>>,
    phantom: PhantomData<S>,
    /// If target feedback is not shared, do not count it multiple times!
    single_state: bool,
}

impl<MT, S> Debug for LibAFLStarManager<MT, S>
where
    MT: Debug,
    S: UsesInput,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LibAFLStarManager")
            //.field("custom_buf_handlers", self.custom_buf_handlers)
            .field("monitor", &self.monitor)
            .field("events", &self.events)
            .finish_non_exhaustive()
    }
}

impl<MT, S> UsesState for LibAFLStarManager<MT, S>
where
    S: State,
{
    type State = S;
}

impl<MT, S> EventFirer for LibAFLStarManager<MT, S>
where
    MT: Monitor,
    S: MultipleStates,
{
    fn fire(
        &mut self,
        state: &mut Self::State,
        event: Event<<Self::State as UsesInput>::Input>,
    ) -> Result<(), Error> {
        let state_idx = if self.single_state {
            0
        } else {
            state.current_state_idx().0
        };
        // clientid 0 is often ignored for stats (it's the broker process),
        // do not use it as a client id for target states.
        let client_id = ClientId((state_idx + 1).try_into()?);
        match Self::handle_in_broker(client_id, &mut self.monitor, &event)? {
            BrokerEventResult::Forward => self.events.push(event),
            BrokerEventResult::Handled => (),
        };
        Ok(())
    }
}

impl<MT, S> EventRestarter for LibAFLStarManager<MT, S>
where
    MT: Monitor,
    S: State,
{
}

impl<E, MT, S, Z> EventProcessor<E, Z> for LibAFLStarManager<MT, S>
where
    MT: Monitor,
    S: State,
{
    fn process(
        &mut self,
        _fuzzer: &mut Z,
        state: &mut S,
        _executor: &mut E,
    ) -> Result<usize, Error> {
        let count = self.events.len();
        while let Some(event) = self.events.pop() {
            self.handle_in_client(state, event)?;
        }
        Ok(count)
    }
}

impl<E, MT, S, Z> EventManager<E, Z> for LibAFLStarManager<MT, S>
where
    MT: Monitor,
    S: MultipleStates + HasExecutions + HasLastReportTime + HasMetadata,
{
}

impl<MT, S> HasCustomBufHandlers for LibAFLStarManager<MT, S>
where
    MT: Monitor,
    S: State,
{
    /// Adds a custom buffer handler that will run for each incoming `CustomBuf` event.
    fn add_custom_buf_handler(
        &mut self,
        handler: Box<
            dyn FnMut(&mut Self::State, &str, &[u8]) -> Result<CustomBufEventResult, Error>,
        >,
    ) {
        self.custom_buf_handlers.push(handler);
    }
}

impl<MT, S> ProgressReporter for LibAFLStarManager<MT, S>
where
    MT: Monitor,
    S: MultipleStates + HasExecutions + HasMetadata + HasLastReportTime,
{
}

impl<MT, S> HasEventManagerId for LibAFLStarManager<MT, S>
where
    MT: Monitor,
    S: UsesInput,
{
    fn mgr_id(&self) -> EventManagerId {
        EventManagerId(0)
    }
}

impl<MT, S> LibAFLStarManager<MT, S>
where
    MT: Monitor,
    S: UsesInput,
{
    /// Creates a new [`SimpleEventManager`].
    pub fn new(monitor: MT) -> Self {
        let mut _self = Self {
            monitor,
            events: vec![],
            custom_buf_handlers: vec![],
            phantom: PhantomData,
            single_state: false,
        };
        _self.monitor.client_stats_insert(ClientId(0));
        _self
    }

    pub fn single_corpus(monitor: MT) -> Self {
        let mut _self = Self {
            monitor,
            events: vec![],
            custom_buf_handlers: vec![],
            phantom: PhantomData,
            single_state: true,
        };
        _self.monitor.client_stats_insert(ClientId(0));
        _self
    }

    /// Handle arriving events in the broker
    #[allow(clippy::unnecessary_wraps)]
    fn handle_in_broker(
        client_id: ClientId,
        monitor: &mut MT,
        event: &Event<S::Input>,
    ) -> Result<BrokerEventResult, Error> {
        match event {
            Event::NewTestcase {
                input: _,
                client_config: _,
                exit_kind: _,
                corpus_size,
                observers_buf: _,
                time,
                executions,
                forward_id: _,
            } => {
                monitor.client_stats_insert(client_id);
                monitor
                    .client_stats_mut_for(client_id)
                    .update_corpus_size(*corpus_size as u64);
                monitor
                    .client_stats_mut_for(client_id)
                    .update_executions(*executions as u64, *time);
                monitor.display("Testcase", client_id);
                Ok(BrokerEventResult::Handled)
            }
            Event::UpdateExecStats {
                time,
                executions,
                phantom: _,
            } => {
                // TODO: The monitor buffer should be added on client add.
                monitor.client_stats_insert(client_id);
                let client = monitor.client_stats_mut_for(client_id);

                client.update_executions(*executions as u64, *time);

                Ok(BrokerEventResult::Handled)
            }
            Event::UpdateUserStats {
                name,
                value,
                phantom: _,
            } => {
                monitor.client_stats_insert(client_id);
                monitor
                    .client_stats_mut_for(client_id)
                    .update_user_stats(name.clone(), value.clone());
                monitor.aggregate(name);
                monitor.display("UserStats", client_id);
                Ok(BrokerEventResult::Handled)
            }
            Event::Objective { objective_size } => {
                monitor.client_stats_insert(client_id);
                monitor
                    .client_stats_mut_for(client_id)
                    .update_objective_size(*objective_size as u64);
                monitor.display("Objective", client_id);
                Ok(BrokerEventResult::Handled)
            }
            Event::Log {
                severity_level,
                message,
                phantom: _,
            } => {
                let (_, _) = (message, severity_level);
                log::log!((*severity_level).into(), "{message}");
                Ok(BrokerEventResult::Handled)
            }
            Event::CustomBuf { .. } => Ok(BrokerEventResult::Forward),
            //_ => Ok(BrokerEventResult::Forward),
        }
    }

    // Handle arriving events in the client
    #[allow(clippy::needless_pass_by_value, clippy::unused_self)]
    fn handle_in_client(&mut self, state: &mut S, event: Event<S::Input>) -> Result<(), Error> {
        if let Event::CustomBuf { tag, buf } = &event {
            for handler in &mut self.custom_buf_handlers {
                handler(state, tag, buf)?;
            }
            Ok(())
        } else {
            Err(Error::unknown(format!(
                "Received illegal message that message should not have arrived: {event:?}."
            )))
        }
    }
}
