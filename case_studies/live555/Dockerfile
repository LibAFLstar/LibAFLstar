FROM libaflstar 

WORKDIR /

# Add a new user ubuntu, pass: ubuntu
RUN groupadd ubuntu && useradd -rm -d /home/ubuntu -s /bin/bash -g ubuntu -G sudo -u 1000 ubuntu -p "$(openssl passwd -1 ubuntu)"

# Copy the patch file
COPY ./live555.patch /live555.patch

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update -y

# Install lighttpd dependencies
RUN apt-get -y install sudo \ 
    libssl-dev \
    libcurl4-openssl-dev

# Create the root directory for the server
RUN chmod 777 /tmp

# Copy the tar of live555 and unzip
COPY live.2023.05.10.tar.gz /live.2023.05.10.tar.gz
RUN tar -xvf live.2023.05.10.tar.gz

# Set up environment variables for ASAN
ENV ASAN_OPTIONS='abort_on_error=1:symbolize=0:detect_leaks=0:detect_stack_use_after_return=1:detect_container_overflow=0:poison_array_cookie=0:malloc_fill_byte=0:max_malloc_fill_size=16777216'

# Compile live555
WORKDIR live
RUN patch -p1 < ../live555.patch
RUN ./genMakefiles linux
RUN AFL_USE_ASAN=1 make all

# Copy the corpus
COPY ./corpus /corpus

# Copy the run script
COPY ./run.sh /LibAFLstar/run.sh
# Make it executable
RUN chmod +x /LibAFLstar/run.sh

WORKDIR /LibAFLstar

ENTRYPOINT [ "./run.sh" ]