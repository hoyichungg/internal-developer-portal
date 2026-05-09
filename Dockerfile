FROM node:20-bookworm AS frontend-build

WORKDIR /app/frontend

COPY frontend/package*.json ./
RUN npm ci

COPY frontend ./
RUN npm run build

FROM rust:1.81-bookworm

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends libpq-dev \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install diesel_cli --version 2.1.1 --locked --no-default-features --features postgres \
    && cargo install cargo-watch --version 8.5.3 --locked

COPY . .
COPY --from=frontend-build /app/frontend/dist /app/frontend/dist

CMD ["sh", "-c", "cargo watch --why -x 'run --bin server'"]
