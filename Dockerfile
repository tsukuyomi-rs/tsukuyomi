FROM rust

RUN apt-get update && \
    apt-get install -y rake && \
    rm -rf /var/lib/apt/lists/*

RUN rustup toolchain install stable beta nightly 1.30.0 && \
    rustup component add rustfmt-preview clippy-preview --toolchain stable

WORKDIR /volume
