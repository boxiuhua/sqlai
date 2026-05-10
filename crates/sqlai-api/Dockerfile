# 多阶段：cargo build → debian slim 运行时
FROM m.daocloud.io/docker.io/library/rust:1.83-bookworm AS build

WORKDIR /opt/sqlai

# CN crates 镜像加速；境外用户可删除整段
RUN mkdir -p /usr/local/cargo && \
    printf '[source.crates-io]\nreplace-with = "rsproxy-sparse"\n\n[source.rsproxy-sparse]\nregistry = "sparse+https://rsproxy.cn/index/"\n\n[net]\ngit-fetch-with-cli = true\n' > /usr/local/cargo/config.toml

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY migrations ./migrations

RUN cargo build --release --bin sqlai-api && \
    strip target/release/sqlai-api


FROM m.daocloud.io/docker.io/library/debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /opt/sqlai

COPY --from=build /opt/sqlai/target/release/sqlai-api /usr/local/bin/sqlai-api
COPY --from=build /opt/sqlai/migrations /opt/sqlai/migrations

EXPOSE 8080
ENV RUST_LOG=info,sqlai=debug

CMD ["sqlai-api"]
