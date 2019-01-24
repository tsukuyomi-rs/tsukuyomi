FROM rust:latest

RUN apt-get update && \
    apt-get install -y --no-install-recommends cmake && \
    rm -rf /var/lib/apt/lists/*

RUN cargo install --git https://github.com/alexcrichton/cargo-local-registry.git && \
    rm -rf /usr/local/cargo/registry/

WORKDIR /volume
