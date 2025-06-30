//! Wraps an executor and measures it's performance
//!

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use libafl::{
    executors::{Executor, ExitKind, HasObservers},
    observers::UsesObservers,
    state::UsesState,
};

use crate::executor::ResettableForkserver;

pub struct ExecutorPerf<B> {
    base: B,
    executions: u128,
    cumulative_time: Duration,
    cumulative_time_ok_only: Duration,
    exit_kinds: HashMap<String, usize>,
}

impl<B> UsesState for ExecutorPerf<B>
where
    B: UsesState,
{
    type State = B::State;
}

impl<B> ExecutorPerf<B> {
    pub fn new(base: B) -> Self {
        Self {
            base,
            cumulative_time: Duration::new(0, 0),
            cumulative_time_ok_only: Duration::new(0, 0),
            executions: 0,
            exit_kinds: HashMap::new(),
        }
    }
}

impl<B, EM, Z> Executor<EM, Z> for ExecutorPerf<B>
where
    B: Executor<EM, Z>,
    EM: UsesState<State = B::State>,
    Z: UsesState<State = B::State>,
{
    fn run_target(
        &mut self,
        fuzzer: &mut Z,
        state: &mut Self::State,
        mgr: &mut EM,
        input: &Self::Input,
    ) -> Result<libafl::prelude::ExitKind, libafl::prelude::Error> {
        self.executions += 1;
        let now = Instant::now();
        let r = self.base.run_target(fuzzer, state, mgr, input);
        let elapsed = now.elapsed();
        self.cumulative_time += elapsed;
        log::info!("Scheduler run_target(): {:?}", elapsed);
        if let Ok(exitkind) = r {
            *(self.exit_kinds.entry(format!("{exitkind:?}")).or_insert(0)) += 1;
            if exitkind == ExitKind::Ok {
                self.cumulative_time_ok_only += elapsed;
            }
        }
        r
    }
}

impl<B> ResettableForkserver for ExecutorPerf<B>
where
    B: ResettableForkserver,
{
    fn reset_target_state(&mut self) -> Result<(), libafl::prelude::Error> {
        let now = Instant::now();
        let r = self.base.reset_target_state();
        let elapsed = now.elapsed();
        self.cumulative_time += elapsed;
        log::info!("Scheduler reset_target_state(): {:?}", elapsed);
        r
    }

    fn state_reset_occurred(&mut self) -> bool {
        let now = Instant::now();
        let r = self.base.state_reset_occurred();
        let elapsed = now.elapsed();
        self.cumulative_time += elapsed;
        log::info!("Scheduler state_reset_occurred(): {:?}", elapsed);
        r
    }
}

impl<B> UsesObservers for ExecutorPerf<B>
where
    B: UsesObservers,
{
    type Observers = B::Observers;
}

impl<B> HasObservers for ExecutorPerf<B>
where
    B: HasObservers,
{
    fn observers(&self) -> &Self::Observers {
        self.base.observers()
    }

    fn observers_mut(&mut self) -> &mut Self::Observers {
        self.base.observers_mut()
    }
}

impl<B> Drop for ExecutorPerf<B> {
    fn drop(&mut self) {
        log::info!(
            "Cumulative time spent in {}: {:?}",
            std::any::type_name_of_val(&self),
            self.cumulative_time
        );
        println!(
            "Cumulative time spent in Executor: {:?}",
            self.cumulative_time
        );
        let average = self.cumulative_time.as_nanos() / self.executions;
        let average = Duration::from_nanos(average as u64);
        println!("Average time per 'run_target': {:?}", average);
        println!("Exit kinds: {:?}", self.exit_kinds);

        let average =
            self.cumulative_time_ok_only.as_nanos() / *self.exit_kinds.get("Ok").unwrap() as u128;
        let average = Duration::from_nanos(average as u64);
        println!("Average time per 'run_target', OK only: {:?}", average)
    }
}
