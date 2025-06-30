mod cli;

use std::{
    fs::{File},
    io::{BufReader, Read},
    path::PathBuf,
    process::Command,
    time::{Duration},
};

use clap::Parser;

use libaflstar::{
    event_manager:: LibAFLStarManager,
    executor::{forkserver::ForkserverExecutor, ResettableForkserver, StatefulPersistentExecutor},
    state::{LibAFLStarState, Prefix, PrefixMetadata},
};
use libafl::{
    corpus::{CachedOnDiskCorpus, OnDiskCorpus},
    executors::HasObservers,
    feedback_and_fast, feedback_or,
    feedbacks::{CrashFeedback, MaxMapFeedback, TimeFeedback},
    fuzzer::StdFuzzer,
    inputs::BytesInput,
    monitors::{MultiMonitor, OnDiskJSONMonitor},
    mutators::Tokens,
    observers::{HitcountsMapObserver, StdMapObserver, TimeObserver},
    schedulers::{IndexesLenTimeMinimizerScheduler, QueueScheduler},
    Evaluator, ExecuteInputResult,
};
use libafl_bolts::{
    current_nanos,
    rands::StdRand,
    shmem::{ShMem, ShMemProvider, UnixShMemProvider},
    tuples::{tuple_list, MatchName},
    AsMutSlice, Error, Truncate,
};

#[allow(clippy::similar_names)]
fn main() -> Result<(), Error> {
    env_logger::init();

    const MAP_SIZE: usize = 65536;

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

    let clean_script = cli.clean_script;

    let timeout_duration = Duration::from_millis(cli.timeout);

    let fuzzer_out_dir: PathBuf = cli.in_dir;

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
    let seed_scheduler = IndexesLenTimeMinimizerScheduler::new(QueueScheduler::new());

    // If we should debug the child
    let debug_child = cli.debug_child;

    // Create the executor for the forkserver
    let args = cli.arguments;

    // Kill signal to kill the target:
    let kill_signal = cli.signal;

    let mut tokens = Tokens::new();

    let mut frsv_builder = ForkserverExecutor::builder();
    if let Some(env_vars) = cli.environment_variables {
        frsv_builder = frsv_builder.envs(env_vars);
    }

    let mut fsrv_executor = frsv_builder
        .program(cli.executable)
        .debug_child(debug_child)
        .socket_client_port(cli.target_port)
        .autotokens(&mut tokens)
        .is_persistent(true)
        .timeout(timeout_duration)
        .parse_afl_cmdline(args)
        .coverage_map_size(MAP_SIZE)
        .kill_signal(kill_signal)
        .build(tuple_list!(time_observer, edges_observer))
        .expect("Building forkserver");

    if let Some(dynamic_map_size) = fsrv_executor.coverage_map_size() {
        fsrv_executor
            .observers_mut()
            .match_name_mut::<HitcountsMapObserver<StdMapObserver<'_, u8, false>>>("shared_mem")
            .unwrap()
            .truncate(dynamic_map_size);
    }

    let mut executor = StatefulPersistentExecutor::new(fsrv_executor);

    let corpus =
        vec![
            CachedOnDiskCorpus::<BytesInput>::new(out_dir.join(format!(".states/state0")), 300)
                .unwrap(),
        ];

    let prefixes = vec![Prefix {
        prefix: Vec::new(),
        metadata: PrefixMetadata { outgoing_edges: 0 },
    }];

    // create the LibAFLStarState
    let mut state = LibAFLStarState::new(
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

    // A fuzzer with feedbacks and a corpus scheduler.
    let mut fuzzer = StdFuzzer::new(seed_scheduler, feedback, objective);

    let files: Result<Vec<_>, _> = std::fs::read_dir(fuzzer_out_dir)?.into_iter().collect();
    let mut files = files?;
    files.sort_by(|f1, f2| f1.file_name().cmp(&f2.file_name()));
    let mut csv_buf = String::new();
    //Append the header (timestamp, coverage)
    csv_buf.push_str("timestamp,coverage,current_edges,total_edges\n");
    let num_files = files.len();
    for (i, file) in files.into_iter().enumerate() {
        let file = file.path();
        if file.is_dir() || file.ends_with("trace_0.cbor") {
            continue;
        }
        println!(
            "({}/{}) Processing trace file [{:?}]",
            i + 1,
            num_files,
            &file.file_name()
        );
        let mut trace_file = BufReader::new(File::open(file)?);
        let mut new_cov = false;
        loop {
            let mut size = [0u8; 4];
            match trace_file.read_exact(&mut size) {
                Ok(()) => {}     // more messages
                Err(_) => break, // end of file?
            }
            let mut buf = vec![0u8; u32::from_le_bytes(size) as usize];
            trace_file.read_exact(&mut buf)?;
            {
                let input = BytesInput::new(buf);

                let (result, _) =
                    match fuzzer.evaluate_input(&mut state, &mut executor, &mut mgr, input) {
                        Ok(a) => a,
                        Err(e) => {
                            println!("Error occurred: {}", e.to_string());
                            break;
                        }
                    };
                if result != ExecuteInputResult::None {
                    new_cov = true;
                    // Get the coverage from state
                    let cov = state.calculate_total_coverage();
                    // Get the current edges (the first usize) and the total edges (the second usize)
                    let (current_edges, total_edges) = match cov {
                        Ok((numerator, denominator)) => (numerator, denominator),
                        Err(_) => (0, 0), // or any other default value you want to use in case of an error
                    };
                    //Calculate the percentage
                    let percentage = (current_edges as f64 / total_edges as f64) * 100.0;
                    let cov_str = format!("{:.2}%", percentage);
                    let file_metadata = trace_file.get_ref().metadata()?;
                    if let Ok(created) = file_metadata.modified() {
                        let unix_timestamp = created.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                        //Append the timestamp, coverage and the current and total edges
                        csv_buf.push_str(&format!("{},{},{},{}\n", unix_timestamp, cov_str, current_edges, total_edges));
                    } else {
                        // If the creation time is not available, print a warning
                        println!("Warn: Could not get the creation time of the trace file");
                    }
                    
                }
            }
        }
        if !new_cov {
            println!("Warn: This trace got us no new coverage");
        }
        // reset process
        executor.reset_target_state()?;
        // run clean script if available
        if let Some(ref cmd) = clean_script {
            let mut handle = Command::new(cmd).spawn()?;
            handle.wait()?;
        }
    }

    let type_names = vec![
        format!("fuzzer: {}", std::any::type_name_of_val(&fuzzer)),
        format!("executor: {}", std::any::type_name_of_val(&executor)),
        format!("state: {}", std::any::type_name_of_val(&state)),
        format!("manager: {}", std::any::type_name_of_val(&mgr)),
    ];

    state.store_fuzzer_info(
        out_dir.join("total_stats_info.txt"),
        format!("{:?}", cli::Cli::parse()),
        type_names,
    )?;

    // Write the csv to a file
    std::fs::write(out_dir.join("coverage_over_time.csv"), csv_buf)?;

    log::info!("Finished");
    println!("Finished! Cya later");
    Ok(())
}