FROM rust:1.74 as build

WORKDIR /build

COPY apps/ ./apps/
COPY octets/ ./octets/
COPY qlog/ ./qlog/
COPY quiche/ ./quiche/

RUN apt-get update && apt-get install -y cmake && rm -rf /var/lib/apt/lists/*

RUN cargo build --manifest-path apps/Cargo.toml

##
## quiche-base: quiche image for apps
##
FROM debian:latest as quiche-base

RUN apt-get update && apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=build \
     /build/apps/target/debug/quiche-client \
     /build/apps/target/debug/quiche-server \
     /build/apps/target/debug/http3-client \
     /usr/local/bin/

ENV PATH="/usr/local/bin/:${PATH}"
ENV RUST_LOG=info

##
## quiche-qns: quiche image for quic-interop-runner
## https://github.com/marten-seemann/quic-network-simulator
## https://github.com/marten-seemann/quic-interop-runner
##
FROM martenseemann/quic-network-simulator-endpoint:latest as quiche-qns

WORKDIR /quiche

RUN apt-get update && apt-get install -y wait-for-it && rm -rf /var/lib/apt/lists/*

COPY --from=build \
     /build/apps/target/debug/quiche-client \
     /build/apps/target/debug/quiche-server \
     /build/apps/target/debug/http3-client \
     /build/apps/run_endpoint.sh \
     ./

ENV RUST_LOG=error

ENTRYPOINT [ "./run_endpoint.sh" ]
