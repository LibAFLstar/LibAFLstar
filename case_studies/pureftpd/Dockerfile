FROM libaflstar 

WORKDIR /

# Add a new user ubuntu, pass: ubuntu
RUN groupadd ubuntu && useradd -rm -d /home/ubuntu -s /bin/bash -g ubuntu -G sudo -u 1000 ubuntu -p "$(openssl passwd -1 ubuntu)"

# Install libpcap-dev
RUN apt-get install -y libpcap-dev

# Copy the patch file
COPY ./pureftpd.patch /pureftpd.patch

# Download pure-ftpd
ENV CC=afl-cc
RUN git clone https://github.com/jedisct1/pure-ftpd.git
WORKDIR /pure-ftpd
RUN git checkout c21b45f
RUN command patch -p1 < /pureftpd.patch
RUN ./autogen.sh
RUN ./configure --without-privsep
RUN make

# Copy the corpus
COPY ./corpus /corpus

# Copy the run script
COPY ./run.sh /LibAFLstar/run.sh

# Make the run script executable
RUN chmod +x /LibAFLstar/run.sh

WORKDIR /LibAFLstar

ENTRYPOINT [ "./run.sh" ]