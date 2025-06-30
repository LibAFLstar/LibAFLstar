FROM libaflstar 

WORKDIR /

# Add a new user ubuntu, pass: ubuntu
#RUN groupadd ubuntu && useradd -rm -d /home/ubuntu -s /bin/bash -g ubuntu -G sudo -u 1000 ubuntu -p "$(openssl passwd -1 ubuntu)"

# Copy the patch file
COPY ./lighttpd.patch /lighttpd.patch

ENV DEBIAN_FRONTEND=noninteractive

# Install lighttpd dependencies
RUN apt-get update -y
RUN apt-get install -y \
    autoconf \
    automake \
    libtool \
    m4 \
    pkg-config \
    libpcre2-dev \
    zlib1g-dev \
    zlib1g \
    openssl \
    libssl-dev \
    scons

# Create the root directory for the server
RUN chmod 777 /tmp

# Install 

# Set up environment variables for ASAN
ENV ASAN_OPTIONS='abort_on_error=1:symbolize=0:detect_leaks=0:detect_stack_use_after_return=1:detect_container_overflow=0:poison_array_cookie=0:malloc_fill_byte=0:max_malloc_fill_size=16777216'

# Download pure-ftpd
ENV CC=afl-cc
ENV CXX=afl-cc
RUN git clone https://git.lighttpd.net/lighttpd/lighttpd1.4.git lighttpd
WORKDIR /lighttpd 
RUN git checkout 9f38b63cae3e2
RUN git apply /lighttpd.patch
RUN ./autogen.sh  
RUN scons CC=/AFLplusplus/afl-cc CXX=/AFLplusplus/afl-cc -j 4 build_static=1 build_dynamic=0
#RUN ./configure --enable-static --disable-shared
#RUN AFL_USE_ASAN=1 make clean all
RUN mv /lighttpd/sconsbuild/static/build/lighttpd /lighttpd/lighttpd

# Copy the corpus
COPY ./corpus /corpus

# Copy the config file
COPY ./lighttpd.conf /lighttpd.conf

# Copy the run script
COPY ./run.sh /LibAFLstar/run.sh
# Make it executable
RUN chmod +x /LibAFLstar/run.sh

WORKDIR /LibAFLstar

ENTRYPOINT [ "./run.sh" ]