# Deploying img-bed

img-bed runs as one static binary behind nginx or Apache, which terminates TLS. The
whole state is the data directory; everything else can be reinstalled from a
release.

Files in this directory:

| File | Installed as |
| --- | --- |
| `img-bed.service` | `/etc/systemd/system/img-bed.service` |
| `img-bed.toml` | `/etc/img-bed/img-bed.toml` |
| `nginx-img-bed.conf` | `/etc/nginx/sites-available/img.aka.cy` (or `conf.d/img.aka.cy.conf`) |
| `apache-img-bed.conf` | `/etc/apache2/sites-available/img.aka.cy.conf`, if the server runs Apache instead of nginx |

Layout on the server:

```
/opt/img-bed/v0.1.0/img-bed     the binary of one release
/opt/img-bed/v0.1.0/web/        its admin frontend
/opt/img-bed/current -> v0.1.0  the release in use (unit and config point here)
/usr/local/bin/img-bed -> /opt/img-bed/current/img-bed
/etc/img-bed/img-bed.toml       config (root:img-bed, 0640)
/var/lib/img-bed/               data directory (img-bed:img-bed, 0750)
```

Upgrading or rolling back is pointing `current` at another release directory
and restarting.

## Getting a release

Pushing a tag `vX.Y.Z` (equal to the versions in `server/Cargo.toml` and
`web/package.json`) makes the CI workflow (`.github/workflows/ci.yml`) attach
to a GitHub Release:

- `img-bed-vX.Y.Z-x86_64-linux-musl.tar.gz` and `…-aarch64-linux-musl.tar.gz`:
  a statically linked binary (runs on any Linux distribution) plus these
  deploy files;
- `img-bed-vX.Y.Z-web.tar.gz`: the admin frontend, unpacking to `web/`;
- `SHA256SUMS`.

Without a release, build the same artifacts yourself (from macOS or Linux,
with [zig](https://ziglang.org) and `cargo install cargo-zigbuild`):

```sh
rustup target add x86_64-unknown-linux-musl
cd server && cargo zigbuild --release --locked --target x86_64-unknown-linux-musl
# binary: server/target/x86_64-unknown-linux-musl/release/img-bed
cd ../web && pnpm install --frozen-lockfile && pnpm build   # web/dist
```

Building on the VPS with a plain `cargo build --release` works too (it needs
a C compiler for the bundled SQLite).

## Fresh install

Run as root on the VPS. Set `V` to the release and `ARCH` to `x86_64` or
`aarch64`.

```sh
V=v0.1.0 ARCH=x86_64
cd /tmp
base=https://github.com/Doublefire-Chen/img-bed/releases/download/$V
curl -fLO $base/img-bed-$V-$ARCH-linux-musl.tar.gz
curl -fLO $base/img-bed-$V-web.tar.gz
curl -fLO $base/SHA256SUMS
sha256sum --ignore-missing -c SHA256SUMS

# 1. A dedicated system user with no login shell.
useradd --system --user-group --home-dir /var/lib/img-bed --shell /usr/sbin/nologin img-bed

# 2. Program files (owned by root, read-only to the service).
mkdir -p /opt/img-bed/$V
tar -xzf img-bed-$V-$ARCH-linux-musl.tar.gz
install -m 0755 img-bed-$V-$ARCH-linux-musl/img-bed /opt/img-bed/$V/img-bed
tar --no-same-owner -xzf img-bed-$V-web.tar.gz -C /opt/img-bed/$V      # -> /opt/img-bed/$V/web
ln -sfn $V /opt/img-bed/current
ln -sfn /opt/img-bed/current/img-bed /usr/local/bin/img-bed

# 3. Data directory, writable only by img-bed.
install -d -o img-bed -g img-bed -m 0750 /var/lib/img-bed

# 4. Config: review listen, upload_limit_mb and public_url.
install -d -m 0755 /etc/img-bed
install -o root -g img-bed -m 0640 img-bed-$V-$ARCH-linux-musl/img-bed.toml /etc/img-bed/img-bed.toml

# 5. The Owner's username and password (prompts for the password twice).
#    Always run commands as img-bed, never as root, or the database files
#    end up owned by root and the service can't write.
sudo -u img-bed img-bed set-password --username <name> --config /etc/img-bed/img-bed.toml

# 6. The service.
install -m 0644 img-bed-$V-$ARCH-linux-musl/img-bed.service /etc/systemd/system/img-bed.service
systemctl daemon-reload
systemctl enable --now img-bed
systemctl status img-bed            # "img-bed 0.1.0 listening on http://127.0.0.1:8080"
curl -s http://127.0.0.1:8080/api/v1/info

# 7. nginx: disable the old Lsky Pro server block for img.aka.cy first (two
#    blocks with the same server_name conflict), then:
install -m 0644 img-bed-$V-$ARCH-linux-musl/nginx-img-bed.conf /etc/nginx/sites-available/img.aka.cy
ln -sfn /etc/nginx/sites-available/img.aka.cy /etc/nginx/sites-enabled/img.aka.cy
#    Edit the ssl_certificate lines to match the existing certificate.
nginx -t && systemctl reload nginx

# 7'. Or Apache instead of nginx: replace the old Lsky Pro virtual host, then:
a2enmod ssl proxy proxy_http headers rewrite
install -m 0644 img-bed-$V-$ARCH-linux-musl/apache-img-bed.conf /etc/apache2/sites-available/img.aka.cy.conf
a2ensite img.aka.cy
#    Edit the SSLCertificate* lines to match the existing certificate.
apachectl configtest && systemctl reload apache2
```

With Apache, `LimitRequestBody` plays the role of `client_max_body_size` (in
bytes: upload limit + 1 MiB), and `ProxyPass` must keep `nocanon` so
percent-encoded Paths reach img-bed unchanged.

Then open `https://img.aka.cy/admin/`, log in and upload an image.

Logs go to the journal: `journalctl -u img-bed -f`. Set `RUST_LOG` in the
unit (e.g. `Environment=RUST_LOG=img_bed=debug,info`) for more detail. The
unit restarts the server if it exits with an error and confines it: it runs
as `img-bed` with no capabilities, sees `/usr`, `/etc` and `/opt` read-only
and `/home` not at all, and can write only `/var/lib/img-bed`. If you move
`data_dir`, change `ReadWritePaths` to match.

If you change `upload_limit_mb`, set nginx's `client_max_body_size` to that
plus 1M (a request may carry 1 MiB of form overhead), or nginx answers 413
before img-bed sees the upload. If you change `listen`, change the nginx
`upstream` too.

### Behind nginx

- img-bed builds every public link from `public_url`, not from the request's
  `Host` header, so a wrong `Host` can't produce wrong links. nginx passes
  `Host` and `X-Forwarded-*` anyway.
- The session cookie is always `Secure`; img-bed doesn't look at
  `X-Forwarded-Proto`. This is why the admin area must be reached over HTTPS
  (the HTTP server block only redirects).
- Client IPs aren't needed: the failed-login limit (5 per 15 minutes) is one
  global counter, not per IP. The `X-Real-IP` header is passed for future use
  and is not trusted by anything today.
- Don't add security or caching headers in nginx: img-bed sets the headers
  that keep images and the admin area apart on one origin (ADR 0001).

## Upgrade

Migrations run automatically at startup. Before an upgrade that may add
one, take a backup (below): migrations only go forward.

```sh
V=v0.2.0 ARCH=x86_64
# download and verify as in the fresh install, then:
mkdir -p /opt/img-bed/$V
tar -xzf img-bed-$V-$ARCH-linux-musl.tar.gz
install -m 0755 img-bed-$V-$ARCH-linux-musl/img-bed /opt/img-bed/$V/img-bed
tar --no-same-owner -xzf img-bed-$V-web.tar.gz -C /opt/img-bed/$V
ln -sfn $V /opt/img-bed/current          # swap binary and frontend together
systemctl restart img-bed
journalctl -u img-bed -n 20              # check it started
```

Compare the new release's `img-bed.service`, `img-bed.toml` and
`nginx-img-bed.conf` with the installed ones and carry over changes, then
`systemctl daemon-reload` / `nginx -t && systemctl reload nginx`. Keep the
previous release directory for rollback; delete older ones whenever.

Upgrading from a release without login usernames: the login page answers
"no username has been set" until you give the Owner one. Run this once after
the restart (it asks for the password too and ends every session; the old
password may be entered again):

```sh
sudo -u img-bed img-bed set-password --username <name> --config /etc/img-bed/img-bed.toml
```

## Rollback

Point `current` back at the previous release and restart:

```sh
ln -sfn v0.1.0 /opt/img-bed/current
systemctl restart img-bed
```

That is enough when the newer release added no Migration. If it did, the
older binary refuses to start (the database has a Migration it doesn't know)
and an older schema can't be reconstructed: stop the service, restore the data
directory backup taken before the upgrade (uploads made since are lost), then
start it.

## Backup

The data directory is the whole state: `img-bed.db` (with `img-bed.db-wal`
and `img-bed.db-shm` while running; SQLite runs in WAL mode) and the Image
files under `images/`. Its `tmp/` holds only in-flight uploads. The config and
release files can be recreated.

Simplest consistent backup: stop the service for the copy.

```sh
systemctl stop img-bed
tar -czf /root/img-bed-$(date +%F).tar.gz -C /var/lib img-bed
systemctl start img-bed
```

Without stopping: copy the database with SQLite's online backup (never copy a
live `img-bed.db` on its own without its `-wal` file), then the files:

```sh
dest=/root/img-bed-backup/$(date +%F)
mkdir -p $dest
sudo -u img-bed sqlite3 /var/lib/img-bed/img-bed.db ".backup '/tmp/img-bed.db'"
mv /tmp/img-bed.db $dest/img-bed.db
rsync -a /var/lib/img-bed/images $dest/
```

(An Image Permanently Deleted between the two steps leaves a record without a
file in the backup; an upload between them leaves an unreferenced file. Stop
the service when that matters.)

To restore: stop the service, replace `/var/lib/img-bed` with the backup
(remove any stale `img-bed.db-wal`/`-shm` that didn't come from the same
backup), `chown -R img-bed:img-bed /var/lib/img-bed`, start the service.

## Trash purge

The 30-day Trash purge runs inside the server process, at startup and then
hourly; no cron job or timer is needed. Images whose 30 days ran out while the
service was stopped are Permanently Deleted when it next starts.
