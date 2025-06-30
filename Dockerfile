FROM --platform=linux/amd64 ubuntu:20.04

# Environment variables
ENV TZ=Europe/Berlin
ENV DEBIAN_FRONTEND=noninteractive
ENV LLVM_VERSION=18

WORKDIR /

# Install AFL++ dependencies
RUN apt-get update -y && apt-get install -y build-essential wget python3-dev automake cmake git flex bison libglib2.0-dev libpixman-1-dev python3-setuptools cargo libgtk-3-dev ninja-build lsb-release software-properties-common gnupg
RUN wget https://apt.llvm.org/llvm.sh
RUN chmod +x llvm.sh
RUN ./llvm.sh 18
RUN rm llvm.sh
RUN apt-get install -y gcc-$(gcc --version|head -n1|sed 's/\..*//'|sed 's/.* //')-plugin-dev libstdc++-$(gcc --version|head -n1|sed 's/\..*//'|sed 's/.* //')-dev

# Install AFL++
RUN git clone https://github.com/AFLplusplus/AFLplusplus
WORKDIR /AFLplusplus
RUN git checkout 775861ea
ENV LLVM_CONFIG=llvm-config-18
RUN make all
RUN make install

# Install Rust nightly 1.76.0
RUN apt-get install -y curl
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup toolchain install nightly
#RUN rustup install 1.76.0
RUN rustup default nightly-2025-03-08

WORKDIR /LibAFLstar

# Copy the source code
COPY src src
COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

# Build AFLstar
RUN cargo build --release

# Install systat for pidstat
RUN apt install -y sysstat
