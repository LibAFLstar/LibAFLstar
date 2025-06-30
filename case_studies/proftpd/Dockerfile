FROM libaflstar 

WORKDIR /

# Add a new user ubuntu, pass: ubuntu
RUN groupadd ubuntu && useradd -rm -d /home/ubuntu -s /bin/bash -g ubuntu -G sudo -u 1000 ubuntu -p "$(openssl passwd -1 ubuntu)"

# Copy the patch file
COPY ./proftpd.patch /proftpd.patch

# add TCP_NODELAY env variable
ENV TCP_NODELAY=1

# Download proftpd
ENV CC=afl-cc
RUN git clone https://github.com/proftpd/proftpd.git
WORKDIR /proftpd
RUN git checkout 61e621e743346
RUN command patch -p1 < /proftpd.patch
RUN ./configure --enable-devel=nodaemon:nofork
ENV ASAN_OPTIONS='abort_on_error=1:symbolize=0:detect_leaks=0:detect_stack_use_after_return=1:detect_container_overflow=0:poison_array_cookie=0:malloc_fill_byte=0:max_malloc_fill_size=16777216'
ENV AFL_USE_ASAN=1
RUN make -j$(nproc)

WORKDIR /

# Copy the corpus
COPY ./corpus /corpus

#Copy the config file
COPY ./basic.conf /basic.conf

# Copy the run script
COPY ./run.sh /LibAFLstar/run.sh

# Make the run script executable
RUN chmod +x /LibAFLstar/run.sh

# mkdir for the server
WORKDIR /home/ubuntu/ftpshare
#change owner and group to ubuntu
RUN chown -R ubuntu:ubuntu /home/ubuntu/ftpshare

WORKDIR /LibAFLstar

ENTRYPOINT [ "./run.sh" ]