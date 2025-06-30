//! Code that has to do with choosing the next state to focus on.
//!
//! Main trait is the [`StateScheduler`] trait, encoding how to choose the next inner state
//! to fuzz

use std::{collections::HashMap, iter::repeat, marker::PhantomData};

use libafl::{
    events::ProgressReporter,
    feedbacks::MapFeedbackMetadata,
    stages::StagesTuple,
    state::{
        HasCorpus, HasExecutions, HasLastReportTime, HasMetadata, HasNamedMetadata, HasRand,
        UsesState,
    },
    Fuzzer, HasFeedback,
};
use libafl_bolts::{impl_serdeany, rands::Rand, Error};
use serde::{Deserialize, Serialize};

use crate::state::{HasSharedMetadata, MultipleStates, TargetStateIdx};

pub trait StateScheduler {
    /// # TRAIT INTERNAL METHOD
    /// Should never be called, except by implementations of this trait.
    /// There should be some way to enforce this in the type system, but I haven't had time to check
    /// that out.
    fn get_weights<Z, ST, E, EM>(
        &mut self,
        _fuzzer: &mut Z,
        _stages: &mut ST,
        _executor: &mut E,
        state: &mut Z::State,
        _manager: &mut EM,
    ) -> Result<Vec<(TargetStateIdx, usize)>, Error>
    where
        Z: Fuzzer<E, EM, ST> + HasFeedback,
        Z::State: StateTraitsAlias,
        E: UsesState<State = Z::State>,
        EM: ProgressReporter<State = Z::State>,
        ST: StagesTuple<E, EM, Z::State, Z>;

    /// Chooses the next state to focus on.
    fn choose_next_state<Z, ST, E, EM>(
        &mut self,
        fuzzer: &mut Z,
        stages: &mut ST,
        executor: &mut E,
        state: &mut Z::State,
        manager: &mut EM,
    ) -> Result<TargetStateIdx, Error>
    where
        Z: Fuzzer<E, EM, ST> + HasFeedback,
        Z::State: StateTraitsAlias,
        E: UsesState<State = Z::State>,
        EM: ProgressReporter<State = Z::State>,
        ST: StagesTuple<E, EM, Z::State, Z>,
    {
        let weight_pairs = self.get_weights(fuzzer, stages, executor, state, manager)?;
        let idx = weighted_choice(weight_pairs, state.rand_mut());
        Ok(idx)
    }
}

/// Trait alias for many bounds that should be implemented
/// by Structs that implement [`libafl::state::State`].
///
/// This exists solely for coding/refactoring convenience. It keeps the implementations trait bounds concise.
pub trait StateTraitsAlias:
    MultipleStates
    + HasSharedMetadata
    + HasMetadata
    + HasNamedMetadata
    + HasExecutions
    + HasLastReportTime
    + HasCorpus
    + HasRand
{
}

impl<T> StateTraitsAlias for T where
    T: MultipleStates
        + HasSharedMetadata
        + HasMetadata
        + HasNamedMetadata
        + HasExecutions
        + HasLastReportTime
        + HasCorpus
        + HasRand
{
}

/// Basic scheduler that simply cycles through the states
pub struct Cycler;

impl StateScheduler for Cycler {
    fn choose_next_state<Z, ST, E, EM>(
        &mut self,
        _fuzzer: &mut Z,
        _stages: &mut ST,
        _executor: &mut E,
        state: &mut Z::State,
        _manager: &mut EM,
    ) -> Result<TargetStateIdx, Error>
    where
        Z: Fuzzer<E, EM, ST> + HasFeedback,
        Z::State: StateTraitsAlias,
        E: UsesState<State = Z::State>,
        EM: ProgressReporter<State = Z::State>,
        ST: StagesTuple<E, EM, Z::State, Z>,
    {
        Ok(TargetStateIdx(
            (state.current_state_idx().0 + 1) % state.states_len(),
        ))
    }

    fn get_weights<Z, ST, E, EM>(
        &mut self,
        _fuzzer: &mut Z,
        _stages: &mut ST,
        _executor: &mut E,
        _state: &mut Z::State,
        _manager: &mut EM,
    ) -> Result<Vec<(TargetStateIdx, usize)>, Error>
    where
        Z: Fuzzer<E, EM, ST> + HasFeedback,
        Z::State: StateTraitsAlias,
        E: UsesState<State = Z::State>,
        EM: ProgressReporter<State = Z::State>,
        ST: StagesTuple<E, EM, Z::State, Z>,
    {
        unimplemented!("This method of the Cycler state scheduler should never be called.");
    }
}

/// Takes the number of outgoing edges of the FSM as the probability weights to choose
/// the next state. I.e., state with more outgoing edges have a larger chance to be chosen.
pub struct OutgoingEdges;

impl StateScheduler for OutgoingEdges {
    fn get_weights<Z, ST, E, EM>(
        &mut self,
        _fuzzer: &mut Z,
        _stages: &mut ST,
        _executor: &mut E,
        state: &mut Z::State,
        _manager: &mut EM,
    ) -> Result<Vec<(TargetStateIdx, usize)>, Error>
    where
        Z: Fuzzer<E, EM, ST> + HasFeedback,
        Z::State: StateTraitsAlias,
        E: UsesState<State = Z::State>,
        EM: ProgressReporter<State = Z::State>,
        ST: StagesTuple<E, EM, Z::State, Z>,
    {
        Ok(state.map_to_vec(|state| Ok((state.current_state_idx(), state.outgoing_edges())))?)
    }
}

/// Keep track of how the bitmaps change when the we choose a new state. Those states that find new stuff get
/// prioritized.
/// This breaks if not all states are at least tried once first, so this is only exposed via other types,
/// such as [`NoveltySearch`] or [`NoveltySearchAndOutgoingEdges`] that are wrapped by [`UnusedFirst`]
pub struct NoveltySearchInner {
    // disable use of constructor; enforce usage of Self::new(), which is private, so we actually enforce uses of [`NoveltySearch`]
    phantom: PhantomData<()>,
}

/// Keeps track of how the bitmaps change of the states change. Those that find new stuff get
/// prioritized.
pub type NoveltySearch = UnusedFirst<NoveltySearchInner>;

impl NoveltySearch {
    pub fn new<S>(state: &mut S) -> Self
    where
        S: MultipleStates + HasSharedMetadata,
    {
        UnusedFirst(NoveltySearchInner::new(state))
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
struct NoveltyIsBetterMetadata {
    /// Indices count the last time the state was run.
    /// Needed so it can be compared to the new count, finding out the difference
    /// (which is then stored in `novelties`)
    /// key: state idx; value: index count
    pub index_counts: HashMap<TargetStateIdx, usize>,
    /// How many new indexes were found, since the last time the state was chosen
    /// key: state idx; value: new indices found
    pub novelties: HashMap<TargetStateIdx, usize>,
}

impl_serdeany!(NoveltyIsBetterMetadata);

impl NoveltySearchInner {
    fn new<S>(state: &mut S) -> Self
    where
        S: MultipleStates + HasSharedMetadata,
    {
        state
            .for_each(|state| {
                if !state.has_shared_metadata::<NoveltyIsBetterMetadata>() {
                    state.add_shared_metadata(NoveltyIsBetterMetadata::default());
                }
                Ok(())
            })
            .unwrap();

        Self {
            phantom: PhantomData,
        }
    }
}

impl StateScheduler for NoveltySearchInner {
    fn get_weights<Z, ST, E, EM>(
        &mut self,
        _fuzzer: &mut Z,
        _stages: &mut ST,
        _executor: &mut E,
        state: &mut Z::State,
        _manager: &mut EM,
    ) -> Result<Vec<(TargetStateIdx, usize)>, Error>
    where
        Z: Fuzzer<E, EM, ST> + HasFeedback,
        Z::State: StateTraitsAlias,
        E: UsesState<State = Z::State>,
        EM: ProgressReporter<State = Z::State>,
        ST: StagesTuple<E, EM, Z::State, Z>,
    {
        let curr_idx = state.current_state_idx();

        // compute the new index count of the current state
        let history_map = &state
            .named_metadata::<MapFeedbackMetadata<u8>>("mapfeedback_metadata_shared_mem")
            .map_err(|e| Error::illegal_state(
                format!("This state scheduler can only work if the underlying StdMapObserver has the name 
                \"shared_mem\", because it is currently hardcoded: {e}")))?
            .history_map;

        // TODO: Do we care about edges (number of non-zero entries) or do we care about edges+buckets (harder to properly check)?
        let curr_cnt = history_map
            .iter()
            .fold(0usize, |acc, e| if *e != 0 { acc + 1 } else { acc }); // number of non-zero entries, i.e., edges w/o buckets

        let meta = state.shared_metadata_mut::<NoveltyIsBetterMetadata>()?;

        // get the previous count while replacing/storing the new index count
        let prev_cnt = meta.index_counts.insert(curr_idx, curr_cnt).unwrap_or(0);

        match curr_cnt.checked_sub(prev_cnt) {
            Some(r) => {
                meta.novelties.insert(curr_idx, r);
            }
            None => {
                return Err(Error::illegal_state(
                    format!("MapFeedbackMetadata history map got smaller, but it should only increase: was {prev_cnt} is now {curr_cnt}"),
                ))
            }
        }

        let novelties = meta.novelties.clone().into_iter().collect();
        Ok(novelties)
    }
}

/// Composable state scheduler that makes sure that each state is first chosen at least once before deferring
/// scheduling to the inner scheduler.
pub struct UnusedFirst<SS>(pub SS)
where
    SS: StateScheduler;

impl<SS> StateScheduler for UnusedFirst<SS>
where
    SS: StateScheduler,
{
    fn get_weights<Z, ST, E, EM>(
        &mut self,
        _fuzzer: &mut Z,
        _stages: &mut ST,
        _executor: &mut E,
        _state: &mut Z::State,
        _manager: &mut EM,
    ) -> Result<Vec<(TargetStateIdx, usize)>, Error>
    where
        Z: Fuzzer<E, EM, ST> + HasFeedback,
        Z::State: StateTraitsAlias,
        E: UsesState<State = Z::State>,
        EM: ProgressReporter<State = Z::State>,
        ST: StagesTuple<E, EM, Z::State, Z>,
    {
        unimplemented!()
    }

    fn choose_next_state<Z, ST, E, EM>(
        &mut self,
        fuzzer: &mut Z,
        stages: &mut ST,
        executor: &mut E,
        state: &mut Z::State,
        manager: &mut EM,
    ) -> Result<TargetStateIdx, Error>
    where
        Z: Fuzzer<E, EM, ST> + HasFeedback,
        Z::State: StateTraitsAlias,
        E: UsesState<State = Z::State>,
        EM: ProgressReporter<State = Z::State>,
        ST: StagesTuple<E, EM, Z::State, Z>,
    {
        // always execute the inner scheduler, because it might need to update/keep track of
        // some state.
        let inner_res = self
            .0
            .choose_next_state(fuzzer, stages, executor, state, manager);
        let curr_idx = state.current_state_idx();
        for idx in 0..state.states_len() {
            let idx = TargetStateIdx(idx);
            state.switch_state(idx)?;
            if *state.fuzz_cycles() == 0 {
                // probably doesn't matter, but let's leave state as we found it
                // so there are no surprises for the caller
                state.switch_state(curr_idx)?;
                return Ok(idx);
            }
        }
        inner_res
    }
}

/// State scheduling strategy that first uses [`NoveltySearch`] and when this yields no results, i.e., there is no novelty,
/// the weights are based on [`OutgoingEdges`].
pub struct NoveltySearchAndOutgoingEdges {
    novelty_search: NoveltySearchInner,
    outgoing_edges: OutgoingEdges,
}

impl NoveltySearchAndOutgoingEdges {
    pub fn new<S>(state: &mut S) -> UnusedFirst<Self>
    where
        S: MultipleStates + HasSharedMetadata,
    {
        UnusedFirst(NoveltySearchAndOutgoingEdges {
            novelty_search: NoveltySearchInner::new(state),
            outgoing_edges: OutgoingEdges {},
        })
    }
}

impl StateScheduler for NoveltySearchAndOutgoingEdges {
    fn get_weights<Z, ST, E, EM>(
        &mut self,
        fuzzer: &mut Z,
        stages: &mut ST,
        executor: &mut E,
        state: &mut Z::State,
        manager: &mut EM,
    ) -> Result<Vec<(TargetStateIdx, usize)>, Error>
    where
        Z: Fuzzer<E, EM, ST> + HasFeedback,
        Z::State: StateTraitsAlias,
        E: UsesState<State = Z::State>,
        EM: ProgressReporter<State = Z::State>,
        ST: StagesTuple<E, EM, Z::State, Z>,
    {
        let ns_weights = self
            .novelty_search
            .get_weights(fuzzer, stages, executor, state, manager)?;
        if ns_weights.iter().map(|(_, weight)| *weight).sum::<usize>() != 0 {
            Ok(ns_weights)
        } else {
            // if all weights are 0, use outgoing edges instead
            self.outgoing_edges
                .get_weights(fuzzer, stages, executor, state, manager)
        }
    }
}

/// Weighted choice
///
/// I just had to implement this real quick. It's dirty, stupuid and likely slow.
/// That said, it's not a hot loop (hopefully), and the weights are probably not that high.
///
/// Args:
/// `weight_pairs`: Slice of tuples corresponding to the value and weight (value, weight)
///
/// All weights are incremented, to ensure that each entry has at least a small chance to be chosen.
///
/// Panics if the iterator is empty
fn weighted_choice<T: Clone, R: Rand>(
    weight_pairs: impl IntoIterator<Item = (T, usize)>,
    rand: &mut R,
) -> T {
    rand.choose(
        weight_pairs
            .into_iter()
            .flat_map(|(value, weight)| repeat(value).take(weight + 1))
            .collect::<Vec<_>>(),
    )
}
