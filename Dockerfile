FROM rust

RUN apt-get update && \
    apt-get install -y rake && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /volume
