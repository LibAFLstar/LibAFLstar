//! An `Executor` wrapping our modified [`crate::executor::forkserver::ForkserverExecutor`] that
//! can be used for stateful persistent mode fuzzing.
//! It enables resetting the target. This is only useful if the target is ran in AFL persistent mode.

use core::fmt::Debug;
use libafl::events::EventFirer;
use libafl::executors::HasObservers;
use libafl::inputs::{HasTargetBytes, UsesInput};
use libafl::monitors::{UserStats, UserStatsValue};
use libafl::observers::{ObserversTuple, UsesObservers};
use libafl_bolts::impl_serdeany;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

use libafl::prelude::ExitKind;
use libafl::state::{HasExecutions, HasMetadata, State};
use libafl::Error;
use libafl::{executors::Executor, state::UsesState};
use libafl_bolts::shmem::ShMemProvider;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use super::forkserver::ForkserverExecutor;

#[derive(Debug)]
pub struct StatefulPersistentExecutor<OT, S, SP>
where
    SP: ShMemProvider,
{
    executor: ForkserverExecutor<OT, S, SP>,
    /// If the state was reset (i.e., there was a timeout)
    state_reset_occurred: bool,
    /// If the child was reset since the last execution
    child_was_reset: bool,
}

pub trait ResettableForkserver {
    /// Resets the state of the target
    fn reset_target_state(&mut self) -> Result<(), Error>;

    /// Signifies if a reset occurred that was not caused by calling
    /// [`ResettableForkserver::reset_target_state`].
    ///
    /// This is important because we want to control the state of the target,
    /// which is reset after the it's killed.
    ///
    /// Whenever this is called, the `flag` is reset, meaning
    /// that it will only return true once whenever a state reset occurs.
    /// The `flag` is also reset when [`ResettableForkserver::reset_target_state`] is called.
    fn state_reset_occurred(&mut self) -> bool;
}

impl<OT, S, SP> StatefulPersistentExecutor<OT, S, SP>
where
    OT: ObserversTuple<S>,
    S: UsesInput,
    SP: ShMemProvider,
{
    /// Create a new [`StatefulPersistentExecutor`]
    pub fn new(executor: ForkserverExecutor<OT, S, SP>) -> Self {
        Self {
            executor,
            state_reset_occurred: false,
            child_was_reset: false,
        }
    }

    pub fn into_inner(self) -> ForkserverExecutor<OT, S, SP> {
        self.executor
    }
}
impl<OT, S, SP> ResettableForkserver for StatefulPersistentExecutor<OT, S, SP>
where
    OT: ObserversTuple<S>,
    S: UsesInput,
    SP: ShMemProvider,
{
    /// Reset the state of the target by killing it.
    /// The forkserver will fork a new process.
    fn reset_target_state(&mut self) -> Result<(), Error> {
        let timed_out = self.executor.forkserver().last_run_timed_out();
        match self.executor.forkserver().child_pid() {
            Some(child_pid) if timed_out => {
                return Err(Error::illegal_state(format!(
                    "Last execution timed out, but there is still a child_pid set (pid={}).",
                    child_pid
                )));
            }
            Some(child_pid) if !timed_out => {
                // usual path, kill the child
                if child_pid.as_raw() > 0 {
                    let process_group = child_pid.as_raw();
                    let result = kill(Pid::from_raw(process_group), Signal::SIGKILL);
                    if let Err(e) = result {
                        log::info!("Error killing child: {e}");
                    }
                    self.executor.forkserver_mut().set_last_run_timed_out(true);
                    self.executor.forkserver_mut().reset_child_pid();
                }
            }
            None => {
                // reset not needed, because child is already killed in case of timeout
                // or, there is no child (such as when calling this before any testcase
                // has been sent)
            }
            _ => unreachable!("All arms are covered."),
        };
        self.child_was_reset = true;
        self.state_reset_occurred = false;
        Ok(())
    }

    fn state_reset_occurred(&mut self) -> bool {
        let result = self.state_reset_occurred;
        self.state_reset_occurred = false;
        result
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct StatefulPersistentExecutorMeta {
    timeouts: u64,
}

impl StatefulPersistentExecutorMeta {
    fn increment_timeouts(&mut self) {
        self.timeouts += 1;
    }

    fn timeouts(&self) -> u64 {
        self.timeouts
    }
}

impl_serdeany!(StatefulPersistentExecutorMeta);

impl<EM, Z, OT, S, SP> Executor<EM, Z> for StatefulPersistentExecutor<OT, S, SP>
where
    EM: UsesState<State = S> + EventFirer,
    Z: UsesState<State = S>,
    OT: ObserversTuple<S>,
    S: State + HasExecutions + HasMetadata,
    S::Input: HasTargetBytes,
    SP: ShMemProvider,
{
    #[inline]
    fn run_target(
        &mut self,
        fuzzer: &mut Z,
        state: &mut Self::State,
        mgr: &mut EM,
        input: &Self::Input,
    ) -> Result<ExitKind, Error> {
        let result = self.executor.run_target(fuzzer, state, mgr, input);

        if self.child_was_reset {
            // we communicated to the forkserver that the child was killed via
            // forkserver.last_run_timed_out,
            // but we need to reset it to false for the next run, otherwise the
            // forkserver will wait forever for a stopped child
            self.child_was_reset = false;
        }

        if let Ok(ExitKind::Timeout) = result {
            // the prefix needs to be sent again
            log::debug!("Timeout occurred, resetting state");
            self.state_reset_occurred = true;

            // keep track of timeouts
            if !state.has_metadata::<StatefulPersistentExecutorMeta>() {
                state.add_metadata(StatefulPersistentExecutorMeta { timeouts: 0 })
            }
            let meta = state.metadata_mut::<StatefulPersistentExecutorMeta>()?;

            meta.increment_timeouts();
            let timeouts = meta.timeouts();

            // send timeouts events, but not too often
            if timeouts < 20 || timeouts % 20 == 0 {
                mgr.fire(
                    state,
                    libafl::events::Event::UpdateUserStats {
                        name: "timeouts".to_string(),
                        value: UserStats::new(
                            UserStatsValue::Number(timeouts),
                            libafl::monitors::AggregatorOps::Max,
                        ),
                        phantom: PhantomData,
                    },
                )?;
            }
        }
        result
    }
}

impl<OT, S, SP> UsesObservers for StatefulPersistentExecutor<OT, S, SP>
where
    OT: ObserversTuple<S>,
    SP: ShMemProvider,
    S: State,
{
    type Observers = OT;
}

impl<OT, S, SP> HasObservers for StatefulPersistentExecutor<OT, S, SP>
where
    OT: ObserversTuple<S>,
    S: UsesInput,
    SP: ShMemProvider,
    S: State,
{
    #[inline]
    fn observers(&self) -> &Self::Observers {
        self.executor.observers()
    }

    #[inline]
    fn observers_mut(&mut self) -> &mut Self::Observers {
        self.executor.observers_mut()
    }
}

impl<OT, S, SP> UsesState for StatefulPersistentExecutor<OT, S, SP>
where
    S: State,
    SP: ShMemProvider,
{
    type State = S;
}
