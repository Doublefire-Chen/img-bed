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

## Deploy

[deploy/README.md](deploy/README.md) covers running img-bed on a Linux server
with systemd behind nginx (unit, production config and nginx server block in
`deploy/`), plus upgrade, rollback and backup. Pushing a `vX.Y.Z` tag builds
static Linux binaries (x86_64 and aarch64, musl) and the admin frontend and
attaches them to a GitHub Release; CI also runs the server and web checks on
every push and pull request to `main`.

## Configuration

See `server/img-bed.example.toml`. Keys: `listen`, `data_dir` (database and
Image files), `frontend_dir`, `upload_limit_mb` (largest accepted file,
default 20) and `public_url` (scheme and host of the links handed out after
an upload, e.g. `https://img.aka.cy`; defaults to `http://<listen>`). A request
may carry 1 MiB of form overhead on top of the limit, so nginx's
`client_max_body_size` must be at least `upload_limit_mb` + 1M.

## Uploading and serving

The admin area opens on the Upload page: drop files, paste from the clipboard
or pick several at once; each gets its link with copy buttons (URL, Markdown,
HTML, BBCode). The API equivalent is `POST /api/v1/images` with
`multipart/form-data` (Owner session or an API Token, see below), returning
one entry per file in `results`.

Only JPEG, PNG, GIF, WebP, AVIF and SVG are accepted, judged by content. Files
are stored unmodified under `data_dir/images/` by SHA-256, and each new Image
gets a first Path from the Path Template (see Settings; `{random:6}.{ext}` by
default). Every Path
outside the Reserved Prefixes `admin/` and `api/` serves its Image directly
with the headers required by [ADR 0001](docs/adr/0001-admin-on-image-origin.md).

Uploading bytes that are already stored is a Duplicate Upload: nothing new is
created, and the result has `"duplicate": true`, the first upload's date in
`original_uploaded_at`, and the existing Image with all its Paths. The Upload
page shows "Already uploaded on <date>" with copy buttons for each Path. If
that Image is in the Trash, it is Restored with all its Paths and the result
also has `"restored": true` ("Restored from Trash" on the Upload page).

## Library

The Library page lists every Image not in the Trash, newest first, 60 per
page, with a search by Path or original filename (a case-insensitive
substring) and the total Image count and disk usage in its header. Clicking
an Image opens its detail view: preview, size, dimensions, type, SHA-256,
upload date and every Path with copy buttons, the Primary Path marked.

API (Owner session): `GET /api/v1/images?page=1&per_page=60&q=text` returns
`images`, `page`, `per_page`, `total` (matches), `pages` and `stats`
(`image_count`, and `disk_usage` in bytes, which includes the Trash's files);
`GET /api/v1/images/{id}` returns one Image with all its Paths. `GET
/api/v1/info` also carries `image_count` and `disk_usage` for the Owner.

## Paths

In an Image's detail view the Owner can add a Path (e.g. `cat.png`), remove
one (after an in-page confirmation) or make any Path Primary. Every Path serves
the Image directly; a removed Path answers 404 and is free for any Image at
once. An added Path may use letters and digits of any script and `- _ . ~ /`
(at most 255 characters, no leading/trailing or double `/`, no `.`/`..`
segments, nothing under `admin/` or `api/`); percent-encoded input is decoded.

API (Owner session), each answering the updated Image:

- `POST /api/v1/images/{id}/paths` with `{"path": "cat.png"}` (201). Errors:
  400 `invalid_path`, 400 `reserved_path`, 409 `path_taken`, 409
  `path_in_trash` (held by an Image in Trash).
- `DELETE /api/v1/images/{id}/paths/{path}` (the Path percent-encoded as in
  its URL). 409 `last_path` for the Image's only Path. Removing the Primary
  Path makes the oldest remaining Path Primary.
- `PUT /api/v1/images/{id}/primary` with `{"path": "cat.png"}`.

A Path that is not one of the Image's answers 404. The Paths of an Image in
the Trash can't be changed until it is Restored (409 `image_in_trash`).

## Trash

Deleting Images (one from its detail view, or several selected in the
Library, after a confirmation saying how many Images and Paths stop working)
moves them to the Trash: their Paths answer 404 but stay held. The Trash page
lists them with the date each will be Permanently Deleted, 30 days after it
was deleted; the Owner can Restore one (every Path serves it again) or delete
it permanently (record and file erased, Paths free). The server Permanently
Deletes Images trashed over 30 days ago at startup and then hourly.

API (Owner session); bulk requests are all or nothing, at most 1000 ids:

- `DELETE /api/v1/images/{id}` or `POST /api/v1/trash` with `{"ids": [..]}`:
  move to the Trash. Images carry `trashed_at` and `permanent_delete_at`.
- `GET /api/v1/trash?page=&per_page=`: the Trash, most recently deleted first.
- `POST /api/v1/trash/restore` with `{"ids": [..]}`.
- `POST /api/v1/trash/delete` with `{"ids": [..]}` or `DELETE
  /api/v1/trash/{id}`: Permanent Delete; 409 `not_in_trash` for an Image not
  in the Trash.
- `GET /api/v1/images/{id}/file`: the Image's file for previews, also while
  it is in the Trash.

## Settings

The Settings page chooses the Path Template: one of the Presets Short
`{random:6}`, Short with extension `{random:6}.{ext}` (the default), Dated
`{Y}/{m}/{d}/{random:13}.{ext}` and Filename `{name}-{random:4}.{ext}`, or a
Custom template, with an example Path for each. It is stored in the database
and applies from the next upload on; existing Paths never change. Variables:
`{random:N}`, `{hash:N}` (N from 1 to 64), `{Y}`, `{m}`, `{d}`, `{name}`,
`{ext}`; a template must contain `{random:N}` or `{hash:N}`.

API (Owner session):

- `GET /api/v1/settings`: `path_template`, `preset` (its id, or null for
  Custom), `example`, and `presets` (`id`, `name`, `template`, `example`).
- `PUT /api/v1/settings` with `{"path_template": "..."}`: saves it and answers
  like `GET`.
- `POST /api/v1/settings/preview` with `{"path_template": "..."}`: checks it
  without saving and answers `path_template`, `preset` and `example`.

Errors (400): `template_not_unique` (neither `{random:N}` nor `{hash:N}`),
`unknown_variable`, `invalid_template` (anything else, e.g. bad characters,
a missing length, a leading `api/` or `admin/`).

## API Tokens

Tools such as PicGo, ShareX, Typora or a script upload with an API Token. On
the API Tokens page, create a named token; its secret (`imgbed_` followed by
43 characters) is shown once with a copy button and stored only as a SHA-256
hash. The list shows each token's created, last-used (updated at most once a
minute) and revoked dates; Revoke stops a token at once. Revoked tokens stay
listed as revoked, and an active token's name must be unique.

A token can call the upload endpoint and nothing else; every other endpoint
answers 401 to it. Send it as `Authorization: Bearer <secret>`:

```sh
curl -H "Authorization: Bearer imgbed_..." \
  -F "file=@photo.png" \
  https://img.example.com/api/v1/images
```

The response is the same as for a browser upload (Duplicate Upload and
Restore included); the link is at `results[0].image.url`. An unknown or
revoked token gets 401 with code `invalid_token`.

Custom uploaders (PicGo's web-uploader plugin, ShareX custom uploader, etc.):

| Setting | Value |
| --- | --- |
| URL / endpoint | `https://<your host>/api/v1/images`, method `POST` |
| Header | `Authorization: Bearer <secret>` |
| Body | `multipart/form-data`, file form name `file` |
| URL in the response (JSON path) | `results[0].image.url` (ShareX: `{json:results[0].image.url}`) |

API (Owner session; a token can't manage tokens):

- `GET /api/v1/tokens`: `tokens`, each `id`, `name`, `created_at`,
  `last_used_at`, `revoked_at` (unix seconds or null); active first, then
  newest first. Never includes secrets.
- `POST /api/v1/tokens` with `{"name": "..."}` (trimmed, 1–100 characters):
  201 with the token and its `secret`, the only time it is returned.
  `token_name_taken` (409) if an active token has that name.
- `DELETE /api/v1/tokens/{id}`: revokes it and answers the token.

Tokens are not affected by changing the password (`set-password` or the
Settings page), which ends sessions only.

## Password

`img-bed set-password` sets or changes the Owner's password (minimum 8
characters) in the data directory's database and ends every existing session.
There is no first-run web page: until it has been run, nobody can log in. It
reads the password from a terminal prompt, or from the first line of standard
input when that is not a terminal.

Once logged in, the Owner can also change it on the Settings page (current
password, then the new one twice). That signs out every other session; the
current one stays logged in until its original expiry. API:
`PUT /api/v1/session/password` with
`{"current_password": "...", "new_password": "..."}` (Owner session only;
API Tokens are refused) answers `{"ended_sessions": n}`. Errors:
`wrong_current_password` (403), `password_too_short` (400), and
`too_many_attempts` (429): a wrong current password counts toward the same
failed-login limit as logging in (5 per 15 minutes).

## Changing queries or migrations

Migrations live in `server/migrations/` and are applied at startup. After
changing a `query!`, refresh the offline data:

```sh
cd server
export DATABASE_URL="sqlite:$PWD/target/sqlx-dev.db"
sqlx database create && sqlx migrate run
cargo sqlx prepare -- --all-targets
```
