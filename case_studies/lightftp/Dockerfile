FROM libaflstar

WORKDIR /

# Add a new user ubuntu, pass: ubuntu
RUN groupadd ubuntu && useradd -rm -d /home/ubuntu -s /bin/bash -g ubuntu -G sudo -u 1000 ubuntu -p "$(openssl passwd -1 ubuntu)"

# Copy the patch file
COPY ./LightFTP.patch /LightFTP.patch

ENV DEBIAN_FRONTEND=noninteractive

# Install lighttpd dependencies
RUN apt-get update -y
RUN apt-get install -y libgnutls28-dev

# Create the root directory for the server
WORKDIR /tmp/ftpshare
WORKDIR /

# Download LightFTP
ENV CC=afl-cc
RUN git clone https://github.com/hfiref0x/LightFTP.git
WORKDIR /LightFTP 
RUN git checkout 5980ea1
RUN patch -p1 < ../LightFTP.patch
WORKDIR /LightFTP/Source/Release
RUN AFL_USE_ASAN=1 make clean all

# Copy the corpus
COPY ./corpus /corpus

# Copy the config file
COPY ./fftp.conf /fftp.conf

# Copy the certificate folder
COPY ./certificate /certificate

# Copy the run script
COPY ./run.sh /LibAFLstar/run.sh

# Make the run script executable
RUN chmod +x /LibAFLstar/run.sh

WORKDIR /LibAFLstar

ENTRYPOINT [ "./run.sh" ]