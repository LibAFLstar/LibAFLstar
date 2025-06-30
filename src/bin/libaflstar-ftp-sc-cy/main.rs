mod cli;

use std::{fs::OpenOptions, io::Write, path::PathBuf, time::Duration};

use clap::Parser;

use libaflstar::{
    event_manager::LibAFLStarManager, executor::{forkserver::ForkserverExecutor, StatefulPersistentExecutor}, fuzzer, mutator::FtpLightMutator, replay::RequestResponseCollector, state::{self, LibAFLStarState, MultipleStates}, state_scheduler
};
use libafl::{
    corpus::{CachedOnDiskCorpus, OnDiskCorpus},
    executors::HasObservers,
    feedback_and_fast, feedback_or,
    feedbacks::{CrashFeedback, MaxMapFeedback, TimeFeedback},
    fuzzer::StdFuzzer,
    inputs::{BytesInput, HasTargetBytes},
    monitors::{MultiMonitor, OnDiskJSONMonitor},
    mutators::{scheduled::havoc_mutations, tokens_mutations, StdScheduledMutator, Tokens},
    observers::{HitcountsMapObserver, ObserversTuple, StdMapObserver, TimeObserver},
    schedulers::QueueScheduler,
    stages::mutational::StdMutationalStage,
    state::{HasMetadata, State},
};
use libafl_bolts::{
    current_nanos,
    rands::StdRand,
    shmem::{ShMem, ShMemProvider, UnixShMemProvider},
    tuples::{tuple_list, Merge},
    AsMutSlice, Error, Truncate,
};
use nix::sys::signal::Signal;

const MAP_SIZE: usize = 65536;

#[allow(clippy::similar_names)]
fn main() -> Result<(), Error> {
    env_logger::init();

    let cli = cli::Cli::parse();

    // Get out dir ready
    let out_dir = cli.out_dir;
    if out_dir.exists() {
        if out_dir.read_dir()?.next().is_some() {
            return Err(Error::illegal_argument(format!(
                "OUT_DIR [{}] must be empty or not exist.",
                out_dir.display()
            )));
        }
    } else {
        std::fs::create_dir(&out_dir)?;
    }

    let timeout_duration = Duration::from_millis(cli.timeout);

    let corpus_dir: PathBuf = cli.in_dir;

    // The unix shmem provider supported by AFL++ for shared memory
    let mut shmem_provider = UnixShMemProvider::new().unwrap();

    // The coverage map shared between observer and executor
    let mut shmem = shmem_provider.new_shmem(MAP_SIZE).unwrap();
    // let the forkserver know the shmid
    shmem.write_to_env("__AFL_SHM_ID").unwrap();
    let shmem_buf = shmem.as_mut_slice();

    // Create an observation channel using the signals map
    let edges_observer =
        unsafe { HitcountsMapObserver::new(StdMapObserver::new("shared_mem", shmem_buf)) };

    // Create an observation channel to keep track of the execution time
    let time_observer = TimeObserver::new("time");

    // Feedback to rate the interestingness of an input
    // This one is composed by two Feedbacks in OR
    let mut feedback = feedback_or!(
        // New maximization map feedback linked to the edges observer and the feedback state
        MaxMapFeedback::tracking(&edges_observer, true, false),
        // Time feedback, this one does not need a feedback state
        TimeFeedback::with_observer(&time_observer)
    );

    // A feedback to choose if an input is a solution or not
    // We want to do the same crash deduplication that AFL does
    let mut objective = feedback_and_fast!(
        // Must be a crash
        CrashFeedback::new(),
        // Take it only if trigger new coverage over crashes
        // Uses `with_name` to create a different history from the `MaxMapFeedback` in `feedback` above
        MaxMapFeedback::with_name("mapfeedback_metadata_objective", &edges_observer)
    );

    let monitor = OnDiskJSONMonitor::new(
        out_dir.join("stats.json"),
        MultiMonitor::new(|s| println!("{s}")),
        |_| true,
    );

    // The event manager handle the various events generated during the fuzzing loop
    // such as the notification of the addition of a new item to the corpus
    let mut mgr = LibAFLStarManager::new(monitor);

    // A queue policy to get testcasess from the corpus
    let seed_scheduler = QueueScheduler::new();

    // If we should debug the child
    let debug_child = cli.debug_child;

    // Create the executor for the forkserver
    let args = cli.arguments;

    // Kill signal to kill the target:
    let kill_signal = cli.signal;

    let mut tokens = Tokens::new();

    let collector = Some(RequestResponseCollector::new(&out_dir.join("replay_traces"))?);

    let mut executor = create_forkserver_executor(
        cli.environment_variables.clone(),
        cli.executable.clone(),
        debug_child,
        cli.target_port,
        timeout_duration.clone(),
        args.clone(),
        collector,
        kill_signal.clone(),
        tuple_list!(time_observer, edges_observer),
        Some(&mut tokens),
    );

    let prefixes = state::load_prefixes(&corpus_dir).unwrap();

    let corpus =
        CachedOnDiskCorpus::<BytesInput>::new(out_dir.join(format!(".states/state")), 300).unwrap();

    // create the LibAFLStarState
    let mut state = LibAFLStarState::new_single_corpus(
        // RNG
        StdRand::with_seed(current_nanos()),
        // Corpus that will be evolved, we keep it in memory for performance
        corpus,
        OnDiskCorpus::new(out_dir.join("crashes")).unwrap(),
        // States of the feedbacks.
        // The feedbacks can report the data that should persist in the State.
        &mut feedback,
        // Same for objective feedbacks
        &mut objective,
        prefixes,
    )
    .unwrap();

    let mut state_scheduler = state_scheduler::Cycler;

    // A fuzzer with feedbacks and a corpus scheduler.
    let mut fuzzer = StdFuzzer::new(seed_scheduler, feedback, objective);

    // Load testcases
    state::load_testcases(
        &mut state,
        &mut fuzzer,
        &mut executor,
        &mut mgr,
        &corpus_dir,
    )
    .unwrap();

    state.for_each(|state| {
        state.add_metadata(tokens.clone());
        Ok(())
    })?;

    // Setup a mutational stage with a basic bytes mutator
    let mutator =
        StdScheduledMutator::with_max_stack_pow(havoc_mutations().merge(tokens_mutations()), 6);
    let mut stages = tuple_list!(StdMutationalStage::with_max_iterations(
        FtpLightMutator::new(mutator),
        // we set the max stage iterations to 1, and control the number of times a test case gets
        // executed in a target state by the number of `loops` in `fuzz_loop_with_signal_handling`
        // this way we have full control.
        1
    ));

    log::debug!("Writing README.stats");
    // Before we start, write the README to the out_dir
    let stats_readme = include_str!("../../resources/README.stats");
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(out_dir.join("README.md"))?
        .write_all(stats_readme.as_bytes())?;

    // Fuzzing loop.
    //
    // Recreate the forkserver if TimeOut error occur

    // keep track of the number of forkserver recreations for debugging
    let mut recreations = 0;
    loop {
        match fuzzer::fuzz_loop_with_signal_handling(
            &mut fuzzer,
            &mut stages,
            &mut executor,
            &mut state,
            &mut mgr,
            &mut state_scheduler,
            cli.loops,
        ) {
            // ShuttingDown is code for recreating the forkserver
            Err(Error::ShuttingDown) => {}
            Ok(_) => break,
            Err(e) => {
                log::error!("Quitting due to error: {}", e);
                println!("Quitting due to error: {}", e);
                break;
            }
        };
        let (collector, observers) = executor.into_inner().shutdown();

        println!("Recreating forkserver executor due to TimeOut error");
        log::error!("Recreating forkserver executor due to TimeOut error");
        recreations += 1;

        executor = create_forkserver_executor(
            cli.environment_variables.clone(),
            cli.executable.clone(),
            debug_child,
            cli.target_port,
            timeout_duration.clone(),
            args.clone(),
            collector,
            kill_signal.clone(),
            observers,
            Some(&mut tokens),
        );
    }

    let type_names = vec![
        std::any::type_name_of_val(&fuzzer),
        std::any::type_name_of_val(&stages),
        std::any::type_name_of_val(&executor),
        std::any::type_name_of_val(&state),
        std::any::type_name_of_val(&mgr),
        std::any::type_name_of_val(&state_scheduler),
    ];

    state.store_fuzzer_info(
        out_dir.join("total_stats_info.txt"),
        format!("{:?}", cli::Cli::parse()),
        type_names,
    )?;

    println!("Quitting! Recreated forkserver {recreations} times");
    Ok(())
}

fn create_forkserver_executor<OT, S>(
    env_vars: Option<Vec<(String, String)>>,
    program: String,
    debug_child: bool,
    target_port: u16,
    timeout: Duration,
    args: Vec<String>,
    collector: Option<RequestResponseCollector>,
    signal: Signal,
    observers: OT,
    tokens: Option<&mut Tokens>,
) -> StatefulPersistentExecutor<OT, S, UnixShMemProvider>
where
    OT: ObserversTuple<S>,
    S: State,
    S::Input: HasTargetBytes,
{
    let mut builder = ForkserverExecutor::builder();
    if let Some(env_vars) = env_vars {
        builder = builder.envs(env_vars)
    }
    if let Some(tokens) = tokens {
        builder = builder.autotokens(tokens);
    }

    if let Some(collector) = collector { 
        builder =builder.collect_request_response_pairs(collector);
    }

    let mut fsrv_executor = builder
        .program(program)
        .debug_child(debug_child)
        .socket_client_port(target_port)
        .is_persistent(true)
        .timeout(timeout)
        .parse_afl_cmdline(args)
        .coverage_map_size(MAP_SIZE)
        .kill_signal(signal)
        .build(observers)
        .expect("Building forkserver");

    if let Some(dynamic_map_size) = fsrv_executor.coverage_map_size() {
        fsrv_executor
            .observers_mut()
            .match_name_mut::<HitcountsMapObserver<StdMapObserver<'_, u8, false>>>("shared_mem")
            .unwrap()
            .truncate(dynamic_map_size);
    }

    StatefulPersistentExecutor::new(fsrv_executor)
}
