FROM libaflstar 

WORKDIR /

# Add a new user ubuntu, pass: ubuntu
RUN groupadd ubuntu && useradd -rm -d /home/ubuntu -s /bin/bash -g ubuntu -G sudo -u 1000 ubuntu -p "$(openssl passwd -1 ubuntu)"

# Copy the patch file
COPY ./bftpd.patch /bftpd.patch

# Download bftpd
ENV CC=afl-cc
RUN wget https://sourceforge.net/projects/bftpd/files/bftpd/bftpd-6.1/bftpd-6.1.tar.gz
RUN tar -zxvf bftpd-6.1.tar.gz
RUN rm bftpd-6.1.tar.gz
RUN patch -p1 < ./bftpd.patch
WORKDIR /bftpd
RUN AFL_USE_ASAN=1 make clean all

# Create the root directory for the server
WORKDIR /tmp/ftpshare

WORKDIR /

# Copy the corpus
COPY ./corpus /corpus

#Copy the config file
COPY ./basic.conf /basic.conf

# Copy the run script
COPY ./run.sh /LibAFLstar/run.sh

# Make the run script executable
RUN chmod +x /LibAFLstar/run.sh

WORKDIR /LibAFLstar

ENTRYPOINT [ "./run.sh" ]