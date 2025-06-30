//! Corpus that wrap others to measure the inner component's performance.

use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use libafl::{corpus::Corpus, inputs::UsesInput};
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct CorpusPerf<B> {
    base: B,
    cumulative_time: Cell<Duration>,
}

impl<B> CorpusPerf<B>
where
    B: Corpus,
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

impl<B> UsesInput for CorpusPerf<B>
where
    B: Corpus,
{
    type Input = B::Input;
}

impl<B> Corpus for CorpusPerf<B>
where
    B: Corpus,
{
    fn count(&self) -> usize {
        let now = Instant::now();
        let r = self.base.count();
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus count(): {:?}", elapsed);
        r
    }

    fn add(
        &mut self,
        testcase: libafl::prelude::Testcase<Self::Input>,
    ) -> Result<libafl::prelude::CorpusId, libafl::prelude::Error> {
        let now = Instant::now();
        let r = self.base.add(testcase);
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus add(): {:?}", elapsed);
        r
    }

    fn replace(
        &mut self,
        idx: libafl::prelude::CorpusId,
        testcase: libafl::prelude::Testcase<Self::Input>,
    ) -> Result<libafl::prelude::Testcase<Self::Input>, libafl::prelude::Error> {
        let now = Instant::now();
        let r = self.base.replace(idx, testcase);
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus replace(): {:?}", elapsed);
        r
    }

    fn remove(
        &mut self,
        id: libafl::prelude::CorpusId,
    ) -> Result<libafl::prelude::Testcase<Self::Input>, libafl::prelude::Error> {
        let now = Instant::now();
        let r = self.base.remove(id);
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus remove(): {:?}", elapsed);
        r
    }

    fn get(
        &self,
        id: libafl::prelude::CorpusId,
    ) -> Result<&std::cell::RefCell<libafl::prelude::Testcase<Self::Input>>, libafl::prelude::Error>
    {
        let now = Instant::now();
        let r = self.base.get(id);
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus get(): {:?}", elapsed);
        r
    }

    fn current(&self) -> &Option<libafl::prelude::CorpusId> {
        let now = Instant::now();
        let r = self.base.current();
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus current(): {:?}", elapsed);
        r
    }

    fn current_mut(&mut self) -> &mut Option<libafl::prelude::CorpusId> {
        let now = Instant::now();
        let r = self.base.current_mut();
        let elapsed = now.elapsed();
        let old = self.cumulative_time.get();
        self.cumulative_time.replace(elapsed + old);
        log::info!("Corpus current_mut(): {:?}", elapsed);
        r
    }

    fn next(&self, id: libafl::prelude::CorpusId) -> Option<libafl::prelude::CorpusId> {
        let now = Instant::now();
        let r = self.base.next(id);
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus next(): {:?}", elapsed);
        r
    }

    fn prev(&self, id: libafl::prelude::CorpusId) -> Option<libafl::prelude::CorpusId> {
        let now = Instant::now();
        let r = self.base.prev(id);
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus prev(): {:?}", elapsed);
        r
    }

    fn first(&self) -> Option<libafl::prelude::CorpusId> {
        let now = Instant::now();
        let r = self.base.first();
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus first(): {:?}", elapsed);
        r
    }

    fn last(&self) -> Option<libafl::prelude::CorpusId> {
        let now = Instant::now();
        let r = self.base.last();
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus last(): {:?}", elapsed);
        r
    }

    fn load_input_into(
        &self,
        testcase: &mut libafl::prelude::Testcase<Self::Input>,
    ) -> Result<(), libafl::prelude::Error> {
        let now = Instant::now();
        let r = self.base.load_input_into(testcase);
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus load_input_into(): {:?}", elapsed);
        r
    }

    fn store_input_from(
        &self,
        testcase: &libafl::prelude::Testcase<Self::Input>,
    ) -> Result<(), libafl::prelude::Error> {
        let now = Instant::now();
        let r = self.base.store_input_from(testcase);
        let elapsed = now.elapsed();
        self.perf_time(elapsed);
        log::info!("Corpus store_input_from(): {:?}", elapsed);
        r
    }
}

impl<B> Drop for CorpusPerf<B> {
    fn drop(&mut self) {
        log::info!(
            "Cumulative time spent in {}: {:?}",
            std::any::type_name_of_val(&self),
            self.cumulative_time.get()
        );
        println!(
            "Cumulative time spent in Corpus: {:?}",
            self.cumulative_time.get()
        );
    }
}
