# Build context is the repository root (the Cargo workspace), see building.yml:
#   docker compose -f Corpo-Metrics/building.yml up --build
ARG RUST_VERSION=1.90
ARG APP_NAME=corpo_metrics
FROM rust:${RUST_VERSION}-slim-bullseye AS build
ARG APP_NAME
ARG PORT
WORKDIR /app


# thx docker docs :D
#For taking advatange of rust cache
RUN --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
  --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
  --mount=type=bind,source=Corpo-Metrics,target=Corpo-Metrics \
  --mount=type=bind,source=Corpo-Metrics-Chatbot,target=Corpo-Metrics-Chatbot \
  --mount=type=cache,target=/app/target/ \
  --mount=type=cache,target=/usr/local/cargo/registry/ \
  <<EOF
set -e
cargo build --locked --release --package $APP_NAME
cp ./target/release/$APP_NAME /bin/server
EOF

FROM ubuntu:latest AS final

COPY Corpo-Metrics/data/ data/
COPY --from=build /bin/server /bin/

EXPOSE 8080

CMD ["/bin/server"]
