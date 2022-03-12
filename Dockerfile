FROM debian:buster-20200908-slim AS builder
ARG DEPLOYMENT_ENVIRONMENT
RUN apt-get update && apt-get install -y --no-install-recommends curl \
    ca-certificates \
    build-essential \
    openssl \ 
    libssl-dev \
    pkg-config \
    cmake \
    && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /squadov/config
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup toolchain install 1.58.1 && rustup default 1.58.1

COPY lib /squadov/lib
COPY server /squadov/server
COPY tools /squadov/tools
COPY deps /squadov/deps
COPY msa /squadov/msa
COPY lambda /squadov/lambda
COPY config/squadov_$DEPLOYMENT_ENVIRONMENT.toml /squadov/config/config.toml
COPY Cargo.toml /squadov/
COPY Cargo.lock /squadov/
COPY devops/gcp /squadov/gcp

WORKDIR /squadov
RUN cargo build --release --bin squadov_api_server

FROM debian:buster-20200908-slim
ARG DEPLOYMENT_ENVIRONMENT
RUN mkdir -p /squadov/config
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    ffmpeg \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /squadov
COPY --from=builder /squadov/target/release/squadov_api_server .
COPY --from=builder /squadov/config/config.toml ./config 
COPY --from=builder /squadov/gcp ./gcp

RUN mkdir -p /squadov/aws
COPY devops/aws/$DEPLOYMENT_ENVIRONMENT.profile ./aws/api.profile
COPY devops/aws/keys/private_s3_vod_cloudfront.pem ./aws/private_s3_vod_cloudfront.pem
COPY run_api_server.sh ./
ENTRYPOINT ["./run_api_server.sh"]