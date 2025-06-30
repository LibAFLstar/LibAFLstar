# LibAFLstar: Fast and State-aware Protocol Fuzzing
This repository contains the artifact and resources for the paper **LibAFLstar: Fast and State-aware Protocol Fuzzing** to appear at **ESORICS 2025**.
Please, if you use this code, cite the paper as follows:

```bibtex
@inproceedings{libaflstar,
    title = {LibAFLstar: Fast and State-Aware Protocol Fuzzing},
    author = {Daniele, Cristian and Bethe, Timme and Maugeri, Marcello and Continella, Andrea and Poll, Erik},
    year = {2025},
}
```

**Note:** the citation is a placeholder and will be updated once the paper is effectively published.

### Repository structure
- `case_studies`: contains the case studies to run LibAFLstar, including the Dockerfiles and the initial corpus.
- `benchmark`: includes partial results of AFLNet, ChatAFL and LibAFLstar for each case study
- `src`: contains the source code of LibAFLstar.
- `Dockerfile`: contains the Dockerfile to build the fuzzer's image.

### Getting Started
For the sake of reproducibility, we provide some Dockerfiles to build the environment and run the examples.
In the root directory, you can find the Dockerfile to build the fuzzer's image.
In the `case_studies` directory, you can find a Dockerfiles for each case study, requiring the fuzzer's image as a base image.
Once built and running, each case study has a `run.sh` script to run the fuzzer with the desired configuration.

#### How to build the base image
`docker build -t libaflstar .`

#### Build all the case studies
```bash
./build_all.sh
```
This script will build the base image and all the case studies.

#### How to run a case study (e.g. live555)
```bash
docker run -d -v "$(pwd)/benchmark/live555":/results --rm libaflstar_live555 -b mcsm-cy -l 1000 -t 24h
```
This command will run the `live555` case study with the `mcsm-cy` binary, 1000 loops, and a timeout of 24 hours. The results will be stored in the `benchmark/live555` directory.

#### Options for running a case study
##### Configurations
`-b` or `--bin` specifies the specific configuration in which the fuzzer should run.
Every configuration is a combination of the level of state awareness and the state scheduler used by the fuzzer.

###### State awareness options
- `mcmm`: Multiple Corpora and Multiple (edge coverage) Map
- `mcsm`: Multiple Corpora and Single (edge coverage) Map
- `sc`: Single Corpus (and single metadata; implied)

###### State scheduler options
- `cy`: Cycler - the fuzzer will cycle among the states of the protocol, trying to cover all the states.
- `oe`: Outgoing Edges - the fuzzer will focus on prioritising states with more outgoing edges (as identified in the `metadata` file for each state in the corpus).
- `ns`: Novelty Search - the fuzzer will focus on states that have not been explored yet, trying to cover all the states.
- `no`: Novelty Search + Outgoing Edges - the fuzzer will focus on states that have not been explored yet, trying to cover all the states, while also prioritising states with more outgoing edges.

Hence, for example `libaflstar-ftp-mcmm-cy` is LibAFLstar for FTP with the multiple corpora and multiple map with the cycler state scheduler.

### Interpretation of the results
The results for each case study are organised in the `benchmark` directory. For every case study, you will find subfolders containing the outputs of different fuzzing campaigns.

**Structure:**
- Each subdirectory (e.g., `bftpd/`, `lightftp/`) contains:
  - **LibAFLstar results:** Folders named like `mcmm-cy-1000-1h/`, `mcsm-oe-10-1h/`, etc. Each folder corresponds to a specific configuration (bin, loop count, time) and contains the corpus, logs, and statistics for that run.
  - **AFLNet/ChatAFL results:** Files like `out-bftpd-aflnet_1.tar.gz` or `out-bftpd-chatafl_1.tar.gz` are compressed archives of the results from AFLNet and ChatAFL.
  - **Replay folders:** Folders such as `out-aflnet-replay/` and `out-chatafl-replay/` contain the results of replaying traces with LibAFLstar to obtain comparable results.

**Naming convention:**
- Folder names encode the configuration:
  - `mcmm-cy-1000-1h/` means:  
    - **mcmm-cy**: the fuzzer binary/strategy used 
    - **1000**: number of loops
    - **1h**: duration of the run
- Archive names follow the pattern:  
  - `out-[case_study]-[aflnet/chatafl]_1.tar.gz`

**Inside each result folder:**
- You will typically find:
  - `stats.json`: A JSON file containing statistics about the corpus related to time.
  - `total_stats_info.txt`: A text file summarizing the total statistics of the run. Also, at the end, it includes the coverage map.


#### How to use LibAFLstar without Docker
Simply reproduce the steps in the Dockerfiles to build the fuzzer.
Note that it requires a specific version of AFL++ due to some changes in the fork-server mechanism and the nightly build of Rust to compile the fuzzer.

#### Example instructions to run the LightFTP case study
- example: 
    `cargo run --release --bin LibAFLstar-ftp-mcmm-cy -- --in-dir case_studies/lightftp/corpus --out-dir <outdir> --target-port <PORT> --loops 100 -t 300 case_studies/lightftp/<path/to/fftp/bin> case_studies/lightftp/fftp.conf <PORT>`
#### Example instructions to replay the traces
`cargo run --release --bin aflnet-traces-replayer -- --in-dir benchmark/out-lightftp-aflnet/replayable-queue --out-dir out-replay --target-port <PORT> case_studies/lightftp/LightFTP/Source/Release/fftp case_studies/lightftp/fftp.conf <PORT>`

#### Reproduce AFLNet/ChatAFL results
The results of AFLNet and ChatAFL for each case study can be found in the benchmark directory in a tar.gz file.
To replay the traces, you need to copy the tar.gz file to the container:
```bash
docker cp [path_to]/out-[case_study]-aflnet_1.tar.gz [container_id]:/
```
After that, you can replay the traces with the following command:
```bash
docker exec -it [container_id] /bin/bash
cd /
tar -xzvf out-[case_study]-[aflnet/chatafl]_1.tar.gz
cargo run --release --bin aflnet-traces-replayer -- --in-dir out-[case_study]-[aflnet/chatafl]_1/replayable-queue --out-dir out-replay --target-port [port] [copy the last part from the run.sh script]
```
Finally, extract the results from the container:
```bash
docker cp [container_id]:/out-replay .
```
The replayer will generate the `out-replay` directory containing the results of replaying the traces with LibAFLstar.