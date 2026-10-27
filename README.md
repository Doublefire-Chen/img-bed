# img-bed

A self-hosted, single-Owner image host: the Owner uploads images and gets public
links to paste into Markdown, HTML or chat. A Rust server (`server/`) plus a
separately built React admin frontend (`web/`) served under `/admin/`.

See [CONTEXT.md](CONTEXT.md) for the domain language and [docs/adr](docs/adr) for decisions.

## Build

Server (Rust; SQL queries are checked at compile time against the committed
`server/.sqlx/` data, so no database is needed to build):

```sh
cd server
cargo build --release          # binary: target/release/img-bed
cargo test
```

Admin frontend (Node + pnpm):

```sh
cd web
pnpm install
pnpm build                     # output: web/dist
pnpm typecheck
```

Keep `version` in `web/package.json` equal to `version` in `server/Cargo.toml`;
the admin area shows a warning banner when they differ.

## Run locally

```sh
cd server
cp img-bed.example.toml img-bed.toml      # paths are relative to this file
cargo run -- set-password                 # prompts twice; or: echo 'pw' | cargo run -- set-password
cargo run -- serve                        # http://127.0.0.1:8080/admin/
```

Every subcommand takes `--config <file>` (default `img-bed.toml`, or `$IMG_BED_CONFIG`).
For frontend development, run `pnpm dev` in `web/` while the server runs; Vite
proxies `/api` to `127.0.0.1:8080`. Open `http://localhost:5173/admin/`.

The session cookie is `Secure`; browsers accept it on `http://localhost`, but in
production serve img-bed over HTTPS (nginx terminates TLS and proxies to `listen`).

## Password

`img-bed set-password` sets or changes the Owner's password (minimum 8
characters) in the data directory's database and ends every existing session.
There is no first-run web page: until it has been run, nobody can log in. It
reads the password from a terminal prompt, or from the first line of standard
input when that is not a terminal.

## Changing queries or migrations

Migrations live in `server/migrations/` and are applied at startup. After
changing a `query!`, refresh the offline data:

```sh
cd server
export DATABASE_URL="sqlite:$PWD/target/sqlx-dev.db"
sqlx database create && sqlx migrate run
cargo sqlx prepare -- --all-targets
```
