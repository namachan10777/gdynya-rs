FROM rust:1.72.0-slim-bullseye as chef
RUN cargo install cargo-chef

FROM chef as plan
WORKDIR /work
COPY . .
RUN cargo chef prepare --recipe-path /work/recipe.json

FROM chef as builder
WORKDIR /work
COPY --from=plan /work/recipe.json recipe.json
RUN cargo chef cook --release --features s3 --recipe-path recipe.json
COPY . .
RUN cargo build --release -p server

FROM debian:bullseye-slim
RUN apt-get update && \
    apt-get install -qy ca-certificates curl && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*
ARG USERNAME=prates
ARG GROUPNAME=prates
ARG UID=1000
ARG GID=1000
RUN groupadd -g $GID $GROUPNAME && \
    useradd -m -s /bin/bash -u $UID -g $GID $USERNAME
USER $USERNAME
COPY --from=builder /work/target/release/server /prates-io
EXPOSE 8080
ENTRYPOINT [ "/prates-io" ]
ENV ADDR=0.0.0.0:8080
ENV RUST_LOG=server=Info
