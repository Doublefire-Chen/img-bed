# Admin area shares the image origin, behind Reserved Prefixes

The admin area (`/admin/`) and API (`/api/`) live on the same host as the images (`img.aka.cy`), with those two prefixes reserved so they can never be Paths. We chose this over a separate admin host (e.g. `admin.img.aka.cy`) to avoid a second domain and certificate.

## Consequences

Uploaded SVGs are served from the same origin as the admin session, so a malicious SVG opened directly could otherwise run script against it. Every image response must therefore carry `Content-Security-Policy: sandbox; default-src 'none'; style-src 'unsafe-inline'` and `X-Content-Type-Options: nosniff`. These headers are load-bearing, not hardening; do not remove them without moving the admin area to its own origin first.
