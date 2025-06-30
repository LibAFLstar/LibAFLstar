//! Seed schedulerthat wrap others to measure the inner component's performance.
//!
//!

use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use libafl::{
    schedulers::Scheduler,
    state::{HasCorpus, UsesState},
};

pub struct SchedulerPerf<B> {
    base: B,
    cumulative_time: Cell<Duration>,
}

impl<B> SchedulerPerf<B>
where
    B: Scheduler,
    B::State: HasCorpus,
{
    pub fn new(base: B) -> Self {
        Self {
            base,
            cumulative_time: Cell::new(Duration::new(0, 0)),
        }
    }

    fn perf_time(&self, time: Duration) {
        let old = self.cumulative_time.get();
        self.cumulative_time.replace(old + time);
    }
}

impl<B> UsesState for SchedulerPerf<B>
where
    B: Scheduler,
    B::State: HasCorpus,
{
    type State = B::State;
}

impl<B> Scheduler for SchedulerPerf<B>
where
    B: Scheduler,
    B::State: HasCorpus,
{
    fn on_add(
        &mut self,
        state: &mut Self::State,
        idx: libafl::prelude::CorpusId,
    ) -> Result<(), libafl::prelude::Error> {
        let now = Instant::now();
        let r = self.base.on_add(state, idx);
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Scheduler on_add(): {:?}", elapsed);
        r
    }

    fn next(
        &mut self,
        state: &mut Self::State,
    ) -> Result<libafl::prelude::CorpusId, libafl::prelude::Error> {
        let now = Instant::now();
        let r = self.base.next(state);
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Scheduler next(): {:?}", elapsed);
        r
    }
}

impl<B> Drop for SchedulerPerf<B> {
    fn drop(&mut self) {
        log::info!(
            "Cumulative time spent in {}: {:?}",
            std::any::type_name_of_val(&self),
            self.cumulative_time.get()
        );
        println!(
            "Cumulative time spent in Scheduler: {:?}",
            self.cumulative_time.get()
        );
    }
}
