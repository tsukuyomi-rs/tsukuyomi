FROM rust:latest as build-env

RUN apt-get update && \
    apt-get install -y \
        cmake \
        binutils-dev \
        libcurl4-openssl-dev \
        zlib1g-dev \
        libdw-dev \
        libiberty-dev

WORKDIR /volume

RUN mkdir kcov-src && \
    curl -sSLf https://github.com/SimonKagstrom/kcov/archive/v33.tar.gz | tar xzf - --strip-components 1 -C kcov-src

RUN mkdir kcov-build && cd kcov-build && \
    cmake -DCMAKE_INSTALL_PREFIX=/usr/local ../kcov-src && \
    make -j2 && \
    make install DESTDIR=../kcov

FROM rust:latest
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        binutils-dev \
        libcurl4-openssl-dev \
        zlib1g-dev \
        libdw-dev \
        libiberty-dev && \
    rm -rf /var/lib/apt/lists/*
COPY --from=build-env /volume/kcov/ /
