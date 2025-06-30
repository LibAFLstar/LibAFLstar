//! Functionality that has to do with the state.
//!
//! The core is the LibAFLStarState, that hold multiple states, with a currently selected state.
//! At all times, LibAFLStarState transparently acts as if it's a single state, so that it can be used with components of LibAFL.
//! How this state is accessed depends on the [`StateAccessMode`]
//! 
//! The main trait here is [`MultipleStates`], implemented by [`LibAFLStarState`]. It defines the major functionality needed for stateful fuzzing

use std::{
    any::type_name,
    cell::{Ref, RefMut},
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    marker::PhantomData,
    path::Path,
    time::Duration,
};

use libafl::{
    corpus::{testcase::Testcase, Corpus, CorpusId, HasCurrentCorpusIdx, HasTestcase},
    events::ProgressReporter,
    feedbacks::{Feedback, MapFeedbackMetadata},
    inputs::{Input, UsesInput},
    stages::{HasCurrentStage, HasNestedStageStatus},
    state::{
        HasCorpus, HasExecutions, HasImported, HasLastReportTime, HasMaxSize, HasMetadata,
        HasNamedMetadata, HasRand, HasSolutions, State, UsesState, DEFAULT_MAX_SIZE,
    },
    Evaluator, ExecuteInputResult,
};
use libafl_bolts::{
    rands::Rand,
    serdeany::{NamedSerdeAnyMap, SerdeAny, SerdeAnyMap},
    Error,
};
use serde::{Deserialize, Serialize};

use crate::{executor::ResettableForkserver, fuzzer};

/// Depending on the mode, components accessing this state get different information.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum StateAccessMode {
    /// The state holds a single corpus, and a single metadata map. All components accessing the corpus and metadata 
    /// get the same data, regardless of the currently selected target state.
    SingleCorp,
    /// The state holds multiple corpora (one for each target state) but a single metadata map. Components accessing 
    /// the corpus get the corpus related to the currently selected target state, but get the same metadata map regardless of 
    /// the currently selected target state.
    MultiCorpSingleMeta,
    /// The state molds multiple corpora and multiple metadata maps. All components accessing the corpus or metadata get a corpus 
    /// or metadata map related to the currently selected target state. 
    MultiCorpMultiMeta,
}

/// Reads the directory and loads the prefixes and corresponding metadata.
/// 
/// - `in_dir`: Path to the input directory
pub fn load_prefixes<C>(in_dir: &Path) -> Result<Vec<Prefix<C>>, Error>
where
    C: Corpus,
{
    // Read the input directory, split into dirs and files
    let mut prefix_dirs = Vec::new();

    let mut dir: Vec<_> = in_dir.read_dir()?.filter_map(Result::ok).collect();
    dir.sort_by_key(|p| p.path());
    for entry in dir {
        if let Ok(file_type) = entry.file_type() {
            match file_type {
                file_type if file_type.is_dir() => prefix_dirs.push(entry),
                _ => {}
            }
        }
    }

    // Read prefixes
    let mut prefixes = Vec::new();
    for dir in prefix_dirs {
        log::info!("Loading prefix with dir name: {:?}", dir.path());

        let mut prefix = Vec::new();
        let mut metadata = None;

        let mut prefix_files = dir.path().read_dir()?.collect::<Result<Vec<_>, _>>()?;
        prefix_files.sort_by_key(|f| f.path());

        for file in prefix_files {
            // metadata file?
            if file.file_name() == "metadata" {
                // if the file gets more complex, this should probably become JSON
                let meta = fs::read_to_string(file.path())?;
                let outgoing_edges = meta.trim().parse::<usize>().map_err(|e| {
                    Error::illegal_state(format!(
                        "Could not parse prefix metadata in {}: {}",
                        dir.path().to_string_lossy(),
                        e
                    ))
                })?;

                metadata = Some(PrefixMetadata { outgoing_edges });
            } else {
                match <C::Input>::from_file(file.path()) {
                    Ok(input) => {
                        let testcase = Testcase::with_filename(
                            input,
                            file.path().to_string_lossy().to_string(),
                        );
                        log::debug!("Loaded prefix input: {:?}", testcase);
                        prefix.push(testcase);
                    }
                    Err(e) => {
                        return Err(Error::illegal_state(format!(
                            "Failed to load a prefix message, this is never correct: {e}"
                        )))
                    }
                };
            }
        }
        if let Some(meta) = metadata {
            prefixes.push(Prefix {
                prefix,
                metadata: meta,
            })
        } else {
            return Err(Error::illegal_state(format!(
                "No metadata file found in prefix dir {}",
                dir.path().to_string_lossy()
            )));
        }
    }

    log::info!(
        "Loaded {} prefixes of the following lengths: {:?}",
        prefixes.len(),
        prefixes.iter().map(|p| p.prefix.len()).collect::<Vec<_>>()
    );

    Ok(prefixes)
}

/// Load the test cases into the state.
/// 
/// - `state`: The state, i.e., the LibAFLstar state.
/// - `fuzzer`: Fuzzer, used to run the test cases before adding them to a corpus
/// - `executor`: The executor
/// - `manager`: Event manager
/// - `in_dir`: Path to the input directory
pub fn load_testcases<Z, E, EM>(
    state: &mut Z::State,
    fuzzer: &mut Z,
    executor: &mut E,
    manager: &mut EM,
    in_dir: &Path,
) -> Result<(), Error>
where
    Z: Evaluator<E, EM>,
    Z::State: MultipleStates + HasCorpus + HasMetadata + HasExecutions + HasLastReportTime,
    <<Z as UsesState>::State as HasCorpus>::Corpus: Clone,
    E: UsesState<State = Z::State> + ResettableForkserver,
    EM: UsesState<State = Z::State> + ProgressReporter<State = Z::State>,
{
    // Read the input directory, split into dirs and files
    let mut files = Vec::new();
    for entry in in_dir.read_dir()? {
        let entry = entry?;
        if let Ok(file_type) = entry.file_type() {
            match file_type {
                file_type if file_type.is_file() => files.push(entry),
                _ => {}
            }
        }
    }

    // Read and evaluate the test cases
    for file in files {
        let input = <<Z as UsesState>::State as UsesInput>::Input::from_file(&file.path())?;
        for id in 0..state.states_len() {
            fuzzer::change_target_state(fuzzer, executor, state, manager, TargetStateIdx(id))?;
            let (res, _) = fuzzer.evaluate_input(state, executor, manager, input.clone())?;
            if res == ExecuteInputResult::None {
                log::warn!(
                    "File {:?} was not interesting, skipped for state {}.",
                    &file,
                    state.current_state_idx()
                );
            }
        }
    }
    log::debug!("Finished loading testcases.");
    Ok(())
}

/// A list of testcases that define the messages that need to be send to
/// the target to get it to a particular state.
///
/// And metadata associated to the target state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefix<C>
where
    C: Corpus,
{
    pub prefix: Vec<Testcase<C::Input>>,
    pub metadata: PrefixMetadata,
}

/// Metadata related to the target state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefixMetadata {
    pub outgoing_edges: usize,
}

/// Modified version of the LibAFL state, extended to work with stateful targets.
/// between different inner target states.
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound = "
        C: serde::Serialize + for<'a> serde::Deserialize<'a>,
        SC: serde::Serialize + for<'a> serde::Deserialize<'a>,
        R: serde::Serialize + for<'a> serde::Deserialize<'a>
    ")]
pub struct LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    /// Access mode
    access_mode: StateAccessMode,
    /// Currently selected target state
    idx: TargetStateIdx,
    /// Total number of target states
    num_states: usize,
    /// Named metadata shared across all target states
    shared_named_metadata: NamedSerdeAnyMap,
    /// Metadata shared across all target states
    shared_metadata: SerdeAnyMap,
    /// Corpus shared among all states
    /// Used depending on the [`StateAccessMode`], namely when we only need a single corpus
    corpus: Option<C>,
    inner: Vec<InnerState<C>>,
    /// prefixes for each target state, indexed with the [`TargetStateIdx`]
    prefixes: Vec<Prefix<C>>,
    /// Last report time
    last_report_time: Option<Duration>,
    /// Max testcase size
    max_size: usize,
    /// The rand instance
    rand: R,
    /// The solutions
    solutions: SC,
    phantom: PhantomData<I>,
}

/// Holds data that is specific to a target state.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct InnerState<C> {
    /// Corpus that is specific to this target state.
    /// Only used depending on the [`StateAccessMode`], 
    /// i.e., when each target state has its own corpus
    pub corpus: Option<C>,
    /// Metadata specific to this target state
    /// Used depending on the [`StateAccessMode`],
    /// i.e., when each target state has its own metadata
    pub metadata: Option<SerdeAnyMap>,
    /// Named metadata specific to this target state
    /// Used depending on the [`StateAccessMode`]
    /// i.e., when each target state has its own metadata
    pub named_metadata: Option<NamedSerdeAnyMap>,
    /// Number of imported test cases
    pub imported: usize,
    /// Execution across this state
    pub executions: usize,
    /// Number of times this state is chosen to fuzz
    pub fuzz_cycles: usize,
    /// Number of outgoing edges in the State Machine.
    /// Used for restarting
    pub corpus_idx: Option<CorpusId>,
    /// The stage indexes for each nesting of stages
    /// Used for restarting
    pub stage_idx_stack: Vec<usize>,
    /// The current stage depth
    /// Used for restarting
    pub stage_depth: usize,
}

impl<C> InnerState<C> {
    /// Create a new inner state. 
    fn new(
        corpus: Option<C>,
        metadata: Option<SerdeAnyMap>,
        named_metadata: Option<NamedSerdeAnyMap>,
    ) -> Self {
        Self {
            corpus,
            metadata,
            named_metadata,
            imported: 0,
            executions: 0,
            fuzz_cycles: 0,
            corpus_idx: None,
            stage_idx_stack: Vec::new(),
            stage_depth: 0,
        }
    }
}

/// Identifier for a target state.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Default,
)]
pub struct TargetStateIdx(pub usize);

impl std::fmt::Display for TargetStateIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StateId({})", self.0)
    }
}

/// Shared metadata that is always shared between target states, regardless of the [`StateAccessMode`]
/// 
/// For example, this is always necessary for the state schedulers to store their metadata, but other components can also use it.
pub trait HasSharedMetadata {
    /// Get named metadata that is shared between all target states
    fn shared_metadata_map(&self) -> &SerdeAnyMap;
    /// Get named mutable metadat that is shared between all target states
    fn shared_metadata_map_mut(&mut self) -> &mut SerdeAnyMap;
    /// Add a metadata to the metadata map that is shared between all target states
    fn shared_named_metadata_map(&self) -> &NamedSerdeAnyMap;
    /// Get mutable metadat that is shared between all target states
    fn shared_named_metadata_map_mut(&mut self) -> &mut NamedSerdeAnyMap;
    /// Add a metadata to the metadata map that is shared between all target states
    #[inline]
    fn add_shared_metadata<M>(&mut self, meta: M)
    where
        M: SerdeAny,
    {
        self.shared_metadata_map_mut().insert(meta);
    }

    /// Check for a metadata
    #[inline]
    fn has_shared_metadata<M>(&self) -> bool
    where
        M: SerdeAny,
    {
        self.shared_metadata_map().get::<M>().is_some()
    }

    /// To get metadata
    #[inline]
    fn shared_metadata<M>(&self) -> Result<&M, Error>
    where
        M: SerdeAny,
    {
        self.shared_metadata_map().get::<M>().ok_or_else(|| {
            Error::key_not_found(format!("{} not found", core::any::type_name::<M>()))
        })
    }
    /// To get metadata mut
    #[inline]
    fn shared_metadata_mut<M>(&mut self) -> Result<&mut M, Error>
    where
        M: SerdeAny,
    {
        self.shared_metadata_map_mut()
            .get_mut::<M>()
            .ok_or_else(|| {
                Error::key_not_found(format!("{} not found", core::any::type_name::<M>()))
            })
    }
    /// Add a metadata to the metadata map
    #[inline]
    fn add_shared_named_metadata<M>(&mut self, name: &str, meta: M)
    where
        M: SerdeAny,
    {
        self.shared_named_metadata_map_mut().insert(meta, name);
    }

    //// !! Unclear why these won't compile. We do not need them (currently), so ... problem for later.
    //    /// Add a metadata to the metadata map
    //    #[inline]
    //    fn remove_shared_named_metadata<M>(&mut self, name: &str) -> Option<Box<M>>
    //    where
    //        M: SerdeAny,
    //    {
    //        self.shared_named_metadata_map_mut().remove::<M>(name)
    //    }
    //
    //    /// Gets metadata, or inserts it using the given construction function `default`
    //    fn shared_named_metadata_or_insert_with<M>(
    //        &mut self,
    //        name: &str,
    //        default: impl FnOnce() -> M,
    //    ) -> &mut M
    //    where
    //        M: SerdeAny,
    //    {
    //        self.shared_named_metadata_map_mut()
    //            .get_or_insert_with::<M>(name, default)
    //    }

    /// Check for a metadata
    ///
    /// # Note
    /// You likely want to use [`Self::named_metadata_or_insert_with`] for performance reasons.
    #[inline]
    fn has_shared_named_metadata<M>(&self, name: &str) -> bool
    where
        M: SerdeAny,
    {
        self.shared_named_metadata_map().contains::<M>(name)
    }

    /// To get named metadata
    #[inline]
    fn shared_named_metadata<M>(&self, name: &str) -> Result<&M, Error>
    where
        M: SerdeAny,
    {
        self.shared_named_metadata_map()
            .get::<M>(name)
            .ok_or_else(|| Error::key_not_found(format!("{} not found", type_name::<M>())))
    }

    /// To get mutable named metadata
    #[inline]
    fn shared_named_metadata_mut<M>(&mut self, name: &str) -> Result<&mut M, Error>
    where
        M: SerdeAny,
    {
        self.shared_named_metadata_map_mut()
            .get_mut::<M>(name)
            .ok_or_else(|| Error::key_not_found(format!("{} not found", type_name::<M>())))
    }
}

/// The main trait enabling stateful fuzzing and focusing of specific target states.
pub trait MultipleStates: State + HasCorpus {
    /// Get the prefix of this state
    fn prefix(&self) -> &Prefix<Self::Corpus>;
    /// Select a different target state that will struct will transparently act as
    fn switch_state(&mut self, idx: TargetStateIdx) -> Result<(), Error>;
    /// Return the index of the currently selected state
    fn current_state_idx(&self) -> TargetStateIdx;
    /// Returns the total number of target states
    fn states_len(&self) -> usize;
    /// Variable keeping track of how often the current target state has been fuzzed
    fn fuzz_cycles(&mut self) -> &mut usize;
    /// Get the number of outgoing edges of this state in the state machine of the SUT.
    /// Arguably, this should be in its own trait. But, meh. Will be refactored if states get more initial metadata
    fn outgoing_edges(&self) -> usize;
    /// Perform a function for each state
    /// 
    /// Execute the closure once for each selected target state. 
    /// In other words, each time the closure is executed, a different target state is selected,
    /// the closure has been executed for each target state.
    ///
    /// Execute the closure on each inner state.
    /// Leaves the state with the same state selected
    /// as with which it was called.
    fn for_each<F>(&mut self, mut f: F) -> Result<(), Error>
    where
        F: FnMut(&mut Self) -> Result<(), Error>,
    {
        let original_idx = self.current_state_idx();
        for idx in 0..self.states_len() {
            self.switch_state(TargetStateIdx(idx))?;
            f(self)?;
        }
        self.switch_state(original_idx)?;
        Ok(())
    }

    /// Convenience function: Produce values from all states.
    /// 
    /// Execute the closure once for each target state that can be selected, producing some value T,
    /// eventually producing a Vec<T>.
    /// Each time the closure is executed, a different target state is selected.
    /// 
    /// Leaves the state with the same state selected
    /// as with which it was called.
    /// 
    /// Can be useful if you need to affect all corpora (such as adding the same test case to each), or all metadata maps.
    fn map_to_vec<F, T>(&mut self, mut f: F) -> Result<Vec<T>, Error>
    where
        F: FnMut(&mut Self) -> Result<T, Error>,
    {
        let original_idx = self.current_state_idx();
        let mut v = Vec::new();
        self.for_each(|state| {
            v.push(f(state)?);
            Ok(())
        })?;
        self.switch_state(original_idx)?;
        Ok(v)
    }
}

impl<I, C, R, SC> LibAFLStarState<I, C, R, SC>
where
    I: Input,
    C: Corpus<Input = <Self as UsesInput>::Input> + Clone,
    R: Rand,
    SC: Corpus<Input = <Self as UsesInput>::Input>,
{
    /// Create a new libaflstar state, specifying the `access_mode`.
    /// 
    /// The number of corpora and prefixes have to be correct, otherwise an error is thrown.
    /// If the access mode is a [`StateAccessMode::SingleCorp`], `corpora` has to be of length 1.
    /// Otherwise, the number of corpora must match the number of prefixes, because we need one for each target state.
    fn new_with_access_mode<F, O>(
        rand: R,
        mut corpora: Vec<C>,
        solutions: SC,
        feedback: &mut F,
        objective: &mut O,
        prefixes: Vec<Prefix<C>>,
        access_mode: StateAccessMode,
    ) -> Result<Self, Error>
    where
        F: Feedback<Self>,
        O: Feedback<Self>,
    {
        if prefixes.len() == 0 {
            return Err(Error::illegal_argument(format!(
                "Length of `prefixes` cannot be 0",
            )));
        }

        match access_mode {
            StateAccessMode::SingleCorp => {
                if corpora.len() != 1 {
                    return Err(Error::illegal_argument(format!(
                        "When using {access_mode:?} access mode, only a single corpus must be supplied. Found {}",
                        corpora.len()
                    )));
                }
            }
            StateAccessMode::MultiCorpMultiMeta | StateAccessMode::MultiCorpSingleMeta => {
                if prefixes.len() != corpora.len() {
                    return Err(Error::illegal_argument(format!(
                        "When using {access_mode:?} access mode, the number of prefixes supplied must be equal to the number of corpora. Found {} corpora and {} prefixes",
                        corpora.len(),
                        prefixes.len()
                    )));
                }
            }
        }

        let num_states = prefixes.len();

        let (shared_corpus, inner) = match access_mode {
            StateAccessMode::SingleCorp => (
                corpora.pop(),
                (0..num_states)
                    .map(|_| InnerState::new(None, None, None))
                    .collect(),
            ),
            StateAccessMode::MultiCorpSingleMeta => {
                let inner = corpora
                    .into_iter()
                    .map(|corpus| InnerState::new(Some(corpus), None, None))
                    .collect();
                (None, inner)
            }
            StateAccessMode::MultiCorpMultiMeta => {
                let inner = corpora
                    .into_iter()
                    .map(|corpus| {
                        InnerState::new(
                            Some(corpus),
                            Some(SerdeAnyMap::new()),
                            Some(NamedSerdeAnyMap::new()),
                        )
                    })
                    .collect();
                (None, inner)
            }
        };
        let mut state = Self {
            idx: TargetStateIdx(0),
            rand,
            solutions,
            num_states,
            inner,
            phantom: PhantomData,
            max_size: DEFAULT_MAX_SIZE,
            last_report_time: None,
            shared_metadata: SerdeAnyMap::new(),
            prefixes,
            access_mode,
            shared_named_metadata: NamedSerdeAnyMap::new(),
            corpus: shared_corpus,
        };

        state.for_each(|inner_state| {
            feedback.init_state(inner_state)?;
            objective.init_state(inner_state)?;
            Ok(())
        })?;

        Ok(state)
    }

    /// Create a new state with a single corpus and a single metadata map for each component.
    pub fn new_single_corpus<F, O>(
        rand: R,
        corpus: C,
        solutions: SC,
        feedback: &mut F,
        objective: &mut O,
        prefixes: Vec<Prefix<C>>,
    ) -> Result<Self, Error>
    where
        F: Feedback<Self>,
        O: Feedback<Self>,
    {
        Self::new_with_access_mode(
            rand,
            vec![corpus],
            solutions,
            feedback,
            objective,
            prefixes,
            StateAccessMode::SingleCorp,
        )
    }

    /// Create a new LibAFLStarState with a corpus for each target state, but shared metadata.
    pub fn new_multi_corpus_single_meta<F, O>(
        rand: R,
        corpora: Vec<C>,
        solutions: SC,
        feedback: &mut F,
        objective: &mut O,
        prefixes: Vec<Prefix<C>>,
    ) -> Result<Self, Error>
    where
        F: Feedback<Self>,
        O: Feedback<Self>,
    {
        Self::new_with_access_mode(
            rand,
            corpora,
            solutions,
            feedback,
            objective,
            prefixes,
            StateAccessMode::MultiCorpSingleMeta,
        )
    }

    /// Create a new LibAFLStarState with a corpus for each target state and it's own metadata for each state.
    /// In other words, everything is separated.
    pub fn new_multi_corpus_multi_meta<F, O>(
        rand: R,
        corpora: Vec<C>,
        solutions: SC,
        feedback: &mut F,
        objective: &mut O,
        prefixes: Vec<Prefix<C>>,
    ) -> Result<Self, Error>
    where
        F: Feedback<Self>,
        O: Feedback<Self>,
    {
        Self::new_with_access_mode(
            rand,
            corpora,
            solutions,
            feedback,
            objective,
            prefixes,
            StateAccessMode::MultiCorpMultiMeta,
        )
    }

    /// Create a new LibAFLStarState with a corpus for each target state and it's own metadata for each state.
    /// In other words, everything is separated.
    ///
    /// This is the same function as [`Self::new_multi_corpus_multi_meta`], as this is the default.
    pub fn new<F, O>(
        rand: R,
        corpora: Vec<C>,
        solutions: SC,
        feedback: &mut F,
        objective: &mut O,
        prefixes: Vec<Prefix<C>>,
    ) -> Result<Self, Error>
    where
        F: Feedback<Self>,
        O: Feedback<Self>,
    {
        Self::new_with_access_mode(
            rand,
            corpora,
            solutions,
            feedback,
            objective,
            prefixes,
            StateAccessMode::MultiCorpMultiMeta,
        )
    }
}

impl<I, C, R, SC> LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    #[inline]
    fn inner(&self) -> &InnerState<C> {
        &self.inner[self.idx.0]
    }

    #[inline]
    fn inner_mut(&mut self) -> &mut InnerState<C> {
        &mut self.inner[self.idx.0]
    }
}

impl<I, C, R, SC> MultipleStates for LibAFLStarState<I, C, R, SC>
where
    I: Input,
    C: Corpus<Input = I>,
    R: Rand,
    SC: Corpus<Input = I>,
{
    #[inline]
    fn prefix(&self) -> &Prefix<C> {
        &self.prefixes[self.idx.0]
    }

    #[inline]
    fn switch_state(&mut self, idx: TargetStateIdx) -> Result<(), Error> {
        if idx.0 > self.num_states - 1 {
            Err(Error::illegal_state(format!("No such state for idx {idx}")))
        } else {
            self.idx = idx;
            Ok(())
        }
    }

    #[inline]
    fn current_state_idx(&self) -> TargetStateIdx {
        self.idx
    }

    #[inline]
    fn states_len(&self) -> usize {
        self.num_states
    }

    #[inline]
    fn fuzz_cycles(&mut self) -> &mut usize {
        &mut self.inner_mut().fuzz_cycles
    }

    fn outgoing_edges(&self) -> usize {
        self.prefix().metadata.outgoing_edges
    }
}

impl<I, C, R, SC> HasSharedMetadata for LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    #[inline]
    fn shared_metadata_map(&self) -> &SerdeAnyMap {
        &self.shared_metadata
    }

    #[inline]
    fn shared_metadata_map_mut(&mut self) -> &mut SerdeAnyMap {
        &mut self.shared_metadata
    }

    #[inline]
    fn shared_named_metadata_map(&self) -> &NamedSerdeAnyMap {
        &self.shared_named_metadata
    }

    #[inline]
    fn shared_named_metadata_map_mut(&mut self) -> &mut NamedSerdeAnyMap {
        &mut self.shared_named_metadata
    }
}

impl<I, C, R, SC> HasCorpus for LibAFLStarState<I, C, R, SC>
where
    I: Input,
    C: Corpus<Input = I>,
    R: Rand,
{
    type Corpus = C;

    #[inline]
    fn corpus(&self) -> &Self::Corpus {
        match self.access_mode {
            StateAccessMode::SingleCorp => self.corpus.as_ref().unwrap(),
            StateAccessMode::MultiCorpMultiMeta | StateAccessMode::MultiCorpSingleMeta => {
                self.inner().corpus.as_ref().unwrap()
            }
        }
    }

    #[inline]
    fn corpus_mut(&mut self) -> &mut Self::Corpus {
        match self.access_mode {
            StateAccessMode::SingleCorp => self.corpus.as_mut().unwrap(),
            StateAccessMode::MultiCorpMultiMeta | StateAccessMode::MultiCorpSingleMeta => {
                self.inner_mut().corpus.as_mut().unwrap()
            }
        }
    }
}

impl<I, C, R, SC> HasMetadata for LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    /// Get all the metadata into an [`hashbrown::HashMap`]
    #[inline]
    fn metadata_map(&self) -> &SerdeAnyMap {
        match self.access_mode {
            StateAccessMode::SingleCorp | StateAccessMode::MultiCorpSingleMeta => {
                &self.shared_metadata
            }
            StateAccessMode::MultiCorpMultiMeta => self.inner().metadata.as_ref().unwrap(),
        }
    }

    /// Get all the metadata into an [`hashbrown::HashMap`] (mutable)
    #[inline]
    fn metadata_map_mut(&mut self) -> &mut SerdeAnyMap {
        match self.access_mode {
            StateAccessMode::SingleCorp | StateAccessMode::MultiCorpSingleMeta => {
                &mut self.shared_metadata
            }
            StateAccessMode::MultiCorpMultiMeta => self.inner_mut().metadata.as_mut().unwrap(),
        }
    }
}

impl<I, C, R, SC> HasNamedMetadata for LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    #[inline]
    fn named_metadata_map(&self) -> &NamedSerdeAnyMap {
        match self.access_mode {
            StateAccessMode::SingleCorp | StateAccessMode::MultiCorpSingleMeta => {
                &self.shared_named_metadata
            }
            StateAccessMode::MultiCorpMultiMeta => self.inner().named_metadata.as_ref().unwrap(),
        }
    }

    #[inline]
    fn named_metadata_map_mut(&mut self) -> &mut NamedSerdeAnyMap {
        match self.access_mode {
            StateAccessMode::SingleCorp | StateAccessMode::MultiCorpSingleMeta => {
                &mut self.shared_named_metadata
            }
            StateAccessMode::MultiCorpMultiMeta => {
                self.inner_mut().named_metadata.as_mut().unwrap()
            }
        }
    }
}

impl<I, C, R, SC> HasImported for LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    fn imported(&self) -> &usize {
        &self.inner().imported
    }

    fn imported_mut(&mut self) -> &mut usize {
        &mut self.inner_mut().imported
    }
}

impl<I, C, R, SC> HasCurrentCorpusIdx for LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    fn set_corpus_idx(&mut self, idx: CorpusId) -> Result<(), Error> {
        self.inner_mut().corpus_idx = Some(idx);
        Ok(())
    }

    fn clear_corpus_idx(&mut self) -> Result<(), Error> {
        self.inner_mut().corpus_idx = None;
        Ok(())
    }

    fn current_corpus_idx(&self) -> Result<Option<CorpusId>, Error> {
        Ok(self.inner().corpus_idx)
    }
}

impl<I, C, R, SC> HasCurrentStage for LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    fn set_stage(&mut self, idx: usize) -> Result<(), Error> {
        // ensure we are in the right frame
        if self.inner().stage_depth != self.inner().stage_idx_stack.len() {
            return Err(Error::illegal_state(
                "stage not resumed before setting stage",
            ));
        }
        self.inner_mut().stage_idx_stack.push(idx);
        Ok(())
    }

    fn clear_stage(&mut self) -> Result<(), Error> {
        self.inner_mut().stage_idx_stack.pop();
        // ensure we are in the right frame
        if self.inner().stage_depth != self.inner().stage_idx_stack.len() {
            return Err(Error::illegal_state(
                "we somehow cleared too many or too few states!",
            ));
        }
        Ok(())
    }

    fn current_stage(&self) -> Result<Option<usize>, Error> {
        Ok(self
            .inner()
            .stage_idx_stack
            .get(self.inner().stage_depth)
            .copied())
    }

    fn on_restart(&mut self) -> Result<(), Error> {
        self.inner_mut().stage_depth = 0; // reset the stage depth so that we may resume inward
        Ok(())
    }
}

impl<I, C, R, SC> HasNestedStageStatus for LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    fn enter_inner_stage(&mut self) -> Result<(), Error> {
        self.inner_mut().stage_depth += 1;
        Ok(())
    }

    fn exit_inner_stage(&mut self) -> Result<(), Error> {
        self.inner_mut().stage_depth -= 1;
        Ok(())
    }
}

impl<I, C, R, SC> State for LibAFLStarState<I, C, R, SC>
where
    I: Input,
    C: Corpus<Input = Self::Input>,
    R: Rand,
    SC: Corpus<Input = Self::Input>,
{
}

impl<I, C, R, SC> HasSolutions for LibAFLStarState<I, C, R, SC>
where
    I: Input,
    SC: Corpus<Input = <Self as UsesInput>::Input>,
    C: Corpus,
{
    type Solutions = SC;

    /// Returns the solutions corpus
    #[inline]
    fn solutions(&self) -> &SC {
        &self.solutions
    }

    /// Returns the solutions corpus (mutable)
    #[inline]
    fn solutions_mut(&mut self) -> &mut SC {
        &mut self.solutions
    }
}

impl<I, C, R, SC> HasRand for LibAFLStarState<I, C, R, SC>
where
    R: Rand,
    C: Corpus,
{
    type Rand = R;

    /// The rand instance
    #[inline]
    fn rand(&self) -> &Self::Rand {
        &self.rand
    }

    /// The rand instance (mutable)
    #[inline]
    fn rand_mut(&mut self) -> &mut Self::Rand {
        &mut self.rand
    }
}

impl<I, C, R, SC> UsesInput for LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
    I: Input,
{
    type Input = I;
}

impl<I, C, R, SC> HasTestcase for LibAFLStarState<I, C, R, SC>
where
    I: Input,
    C: Corpus<Input = <Self as UsesInput>::Input>,
    R: Rand,
{
    /// To get the testcase
    fn testcase(&self, id: CorpusId) -> Result<Ref<Testcase<<Self as UsesInput>::Input>>, Error> {
        Ok(self.corpus().get(id)?.borrow())
    }

    /// To get mutable testcase
    fn testcase_mut(
        &self,
        id: CorpusId,
    ) -> Result<RefMut<Testcase<<Self as UsesInput>::Input>>, Error> {
        Ok(self.corpus().get(id)?.borrow_mut())
    }
}

impl<I, C, R, SC> HasMaxSize for LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    fn max_size(&self) -> usize {
        self.max_size
    }

    fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size;
    }
}

impl<I, C, R, SC> HasLastReportTime for LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    fn last_report_time(&self) -> &Option<Duration> {
        &self.last_report_time
    }

    fn last_report_time_mut(&mut self) -> &mut Option<Duration> {
        &mut self.last_report_time
    }
}

impl<I, C, R, SC> HasExecutions for LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    fn executions(&self) -> &usize {
        &self.inner().executions
    }

    fn executions_mut(&mut self) -> &mut usize {
        &mut self.inner_mut().executions
    }
}

impl<I, C, R, SC> LibAFLStarState<I, C, R, SC>
where
    C: Corpus,
{
    /// Store stats about the whole fuzzer state to a file.
    /// Meant to be called just before exiting
    /// 
    /// ONLY WORKS WITH THE PROVIDED BINARIES, because it accesses Named Metadata of components, which otherwise will not exist or match up.
    pub fn store_fuzzer_info(
        &self,
        path: impl AsRef<Path>,
        cli_options: String,
        type_names: impl std::fmt::Debug,
    ) -> Result<(), Error> {
        let mut writer = BufWriter::new(
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(path.as_ref())?,
        );

        writer.write_all(format!("cli_options: {}\n", cli_options).as_bytes())?;

        // Total coverage.
        let (cov, total) = self.calculate_total_coverage()?;
        let perc = (cov as f32 / total as f32) * 100f32;
        writer.write_all(
            format!("complete_coverage: {:.0}% ({}/{})\n", perc, cov, total).as_bytes(),
        )?;

        // Write cycles
        let executions_per_state = self
            .inner
            .iter()
            .enumerate()
            .map(|(id, inner)| (id, inner.executions))
            .collect::<Vec<_>>();
        let total_executions: usize = executions_per_state.iter().map(|(_, exec)| exec).sum();
        writer.write_all(
            format!(
                "executions_per_state: (id, #exec): {:?}\n",
                executions_per_state
            )
            .as_bytes(),
        )?;
        writer.write_all(format!("total_executions: {:?}\n", total_executions).as_bytes())?;
        let cycles = self
            .inner
            .iter()
            .enumerate()
            .map(|(id, inner)| (id, inner.fuzz_cycles))
            .collect::<Vec<_>>();
        writer.write_all(format!("cycles_per_state (id, #cycles): {:?}\n", cycles).as_bytes())?;

        writer.write_all(format!("type_names: {:#?}\n", type_names).as_bytes())?;

        //Write the coverage map as bytes
        writer.write_all(format!("Coverage map: {:#?}\n", self.get_coverage_map_as_bytes()).as_bytes())?;

        Ok(())
    }

    /// Helper function for [`LibAFLStarState::store_fuzzer_info`]
    /// 
    /// Returns overall coverage as a percentage (a, b) -> a over b.
    pub fn calculate_total_coverage(&self) -> Result<(usize, usize), Error> {
        let mut total_map = Vec::new();
        match self.access_mode {
            // there is only a single bitmap, just get it.
            StateAccessMode::SingleCorp | StateAccessMode::MultiCorpSingleMeta => {
                let map = &self.named_metadata_map().get::<MapFeedbackMetadata<u8>> ("mapfeedback_metadata_shared_mem")
                    .ok_or_else(
                        || Error::illegal_state("Cannot calculate average coverage because different Feedback type was used. Expected MapFeedback<u8>")
                    )?.history_map;
                total_map = map.clone()
            }
            // each target state (inner state) has its own bitmap.
            // merge them into `total_map`
            StateAccessMode::MultiCorpMultiMeta => {
                for state in self.inner.iter() {
                    let map = &state.named_metadata.as_ref().unwrap().get::<MapFeedbackMetadata<u8>> ("mapfeedback_metadata_shared_mem")
                        .ok_or_else(
                            || Error::illegal_state("Cannot calculate average coverage because different Feedback type was used. Expected MapFeedback<u8>")
                        )?.history_map;

                    if total_map.len() < map.len() {
                        total_map.resize(map.len(), 0u8);
                    }
                    for (i, byte) in map.iter().enumerate() {
                        // # Safety
                        // We just resized total_map to be at least as long as map above.
                        let total_map_val = unsafe { total_map.get_unchecked(i) };
                        *unsafe { total_map.get_unchecked_mut(i) } = *total_map_val.max(byte);
                    }
                }
            }
        }

        let coverage = total_map
            .iter()
            .filter(|byte| **byte != 0u8)
            .collect::<Vec<_>>()
            .len();

        Ok((coverage, total_map.len()))
    }

    pub fn get_coverage_map_as_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut total_map = Vec::new();
        match self.access_mode {
            // there is only a single bitmap, just get it.
            StateAccessMode::SingleCorp | StateAccessMode::MultiCorpSingleMeta => {
                let map = &self.named_metadata_map().get::<MapFeedbackMetadata<u8>> ("mapfeedback_metadata_shared_mem")
                    .ok_or_else(
                        || Error::illegal_state("Cannot calculate average coverage because different Feedback type was used. Expected MapFeedback<u8>")
                    )?.history_map;
                total_map = map.clone()
            }
            // each target state (inner state) has its own bitmap.
            // merge them into `total_map`
            StateAccessMode::MultiCorpMultiMeta => {
                for state in self.inner.iter() {
                    let map = &state.named_metadata.as_ref().unwrap().get::<MapFeedbackMetadata<u8>> ("mapfeedback_metadata_shared_mem")
                        .ok_or_else(
                            || Error::illegal_state("Cannot calculate average coverage because different Feedback type was used. Expected MapFeedback<u8>")
                        )?.history_map;

                    if total_map.len() < map.len() {
                        total_map.resize(map.len(), 0u8);
                    }
                    for (i, byte) in map.iter().enumerate() {
                        // # Safety
                        // We just resized total_map to be at least as long as map above.
                        let total_map_val = unsafe { total_map.get_unchecked(i) };
                        *unsafe { total_map.get_unchecked_mut(i) } = *total_map_val.max(byte);
                    }
                }
            }
        }
        Ok(total_map)
    }
}
