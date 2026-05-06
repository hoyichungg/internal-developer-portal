# Internal Developer Portal API

A Rocket + Diesel Async + PostgreSQL API for an internal developer portal. It
includes a software catalog for maintainers and packages, CLI user management,
Argon2 password hashing, session tokens, request validation, and integration
tests for the main REST resources.

## Stack

- Rust 1.81
- Rocket 0.5
- Diesel 2.1 and diesel-async
- PostgreSQL 16
- Docker Compose for local development
- GitHub Actions CI

## Local Development

Start the database and application:

```sh
docker compose up --build
```

The application listens on:

```text
http://127.0.0.1:8000
```

The app container runs database migrations before starting the Rocket server.

## Environment

Copy `.env.example` to `.env` when running tools directly on the host. The
server and CLI load `.env` automatically. The Docker Compose setup already
provides the required environment variables.

Application-level config is loaded from environment variables:

- `APP_ENV`
- `AUTH_TOKEN_TTL_SECONDS`
- `DATABASE_URL`
- `ROCKET_DATABASES`

## CLI

Create a user with roles:

```sh
cargo run --bin cli -- users create alice password123 admin,member
```

List users:

```sh
cargo run --bin cli -- users list
```

Delete a user:

```sh
cargo run --bin cli -- users delete 1
```

## Tests

Integration tests expect the server to be running on `127.0.0.1:8000`.

```sh
cargo test
```

The CI workflow runs formatting, build checks, Clippy, Diesel migrations, a
real Rocket server, and the integration tests against PostgreSQL 16.

## API Shape

Successful JSON responses use a consistent wrapper:

```json
{
  "data": {}
}
```

Errors use a consistent error body:

```json
{
  "error": {
    "code": "validation_failed",
    "message": "Request validation failed.",
    "details": []
  }
}
```

Authenticated routes expect:

```text
Authorization: Bearer <token>
```

## Catalog Domain

- `maintainers` represent teams or people responsible for internal packages.
- `packages` represent cataloged software packages owned by a maintainer.
- Package lifecycle `status` is one of `active`, `deprecated`, or `archived`.
- Packages can link to source repositories and documentation.

## API

- `GET /health`
- `POST /login`
- `GET /me`
- `POST /logout`
- `GET /maintainers`
- `POST /maintainers`
- `GET /maintainers/<id>`
- `PUT /maintainers/<id>`
- `DELETE /maintainers/<id>`
- `GET /packages`
- `POST /packages`
- `GET /packages/<id>`
- `PUT /packages/<id>`
- `DELETE /packages/<id>`
