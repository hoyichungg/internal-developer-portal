FROM rust:1.81-bookworm

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends libpq-dev \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install diesel_cli --version 2.1.1 --locked --no-default-features --features postgres \
    && cargo install cargo-watch --version 8.5.3 --locked

COPY . .

CMD ["sh", "-c", "diesel migration run && cargo watch --why -x 'run --bin server'"]
