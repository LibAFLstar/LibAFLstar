FROM libaflstar

WORKDIR /

RUN git clone https://github.com/FreeOpcUa/freeopcua.git
WORKDIR /freeopcua
RUN git checkout 6eac097
COPY ./freeopcua.patch /freeopcua.patch
RUN patch -p1 < /freeopcua.patch

# Install dependencies
RUN apt-get update && apt-get install -y \
    libtool \
    libboost-dev \
    libboost-thread-dev \
    libboost-program-options-dev \
    libboost-system-dev \
    libboost-filesystem-dev \
    libcppunit-dev \
    libboost-python-dev \
    gsoap \
    libxml2-dev \
    libxslt1-dev \
    autotools-dev \
    dh-make \
    debhelper \
    devscripts \
    fakeroot \
    xutils \
    lintian \
    pbuilder \
    reprepro \
    libmbedtls-dev

ARG CACHE_DATE=not_a_date

ENV CC=afl-cc
ENV CXX=afl-c++

RUN mkdir build
WORKDIR /freeopcua/build
RUN ls
RUN AFL_USE_ASAN=1 cmake -DBUILD_SHARED_LIBS=OFF -DBUILD_CLIENT=OFF -DBUILD_PYTHON=OFF -DBUILD_TESTING=OFF -DSSL_SUPPORT_MBEDTLS=OFF -DBoost_USE_STATIC_LIBS=ON -DCMAKE_CXX_FLAGS="-Wno-enum-constexpr-conversion"  ..
RUN make -j 10

# Copy the run script
COPY ./run.sh /LibAFLstar/run.sh

# Make the run script executable
RUN chmod +x /LibAFLstar/run.sh

WORKDIR /LibAFLstar

# In all the 'main.rs' files in every folder inside src/bin, change the line
#     const MAP_SIZE: usize = 65536;
# to
#     const MAP_SIZE: usize = 114985;
RUN find . -type f -name "main.rs" -exec sed -i 's/const MAP_SIZE: usize = 65536;/const MAP_SIZE: usize = 114985;/' {} +

COPY ./corpus /corpus

ENTRYPOINT [ "./run.sh" ]