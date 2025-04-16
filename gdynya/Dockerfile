FROM rust:1.86.0-slim-bookworm as chef
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
RUN cargo build --release -p gdynya

FROM debian:bookworm-slim
RUN apt-get update && \
    apt-get install -qy ca-certificates curl && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*
ARG USERNAME=gdynya
ARG GROUPNAME=gdynya
ARG UID=1000
ARG GID=1000
RUN groupadd -g $GID $GROUPNAME && \
    useradd -m -s /bin/bash -u $UID -g $GID $USERNAME
USER $USERNAME
COPY --from=builder /work/target/release/gdynya /gdynya
EXPOSE 8080
ENTRYPOINT [ "/gdynya" ]
ENV ADDR=0.0.0.0:8080
ENV RULES=/etc/gdynya/rules.yaml
ENV RUST_LOG=gdynya=Info
