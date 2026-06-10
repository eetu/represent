# Security model — represent

A LAN-only markdown viewer/editor for demo scripts. Durable state is a
directory of plain `.md` files (`REPRESENT_DATA_DIR`, one subdirectory per
project) on a restic-backed bind mount. It holds **no auth secret of its own**
and talks to **no upstream services**.

## Trust boundaries

- **Edge auth is oauth2-proxy, not this app.** On the Pi the host is a Traefik
  *gated host* (`_gated_hosts` in `../raspi/tasks/traefik.py`): every request
  is forced through oauth2-proxy → Kanidm before it reaches the backend. The
  backend trusts the injected `X-Auth-Request-User` / `X-Auth-Request-Email`
  headers and **requires them on every `/api/*` route** (401 if absent) as
  defense-in-depth against a request that bypassed the proxy on the loopback
  port. These headers are PII and are **never logged**.
- **No own login / session / cookie / Kanidm client.** There is no
  `SESSION_KEY`, no signed cookie, no OIDC flow in the binary. Removing that
  surface is the point of sitting behind the forward-auth edge.
- **`DEV_AUTH=1`** bypasses the header gate with a synthetic user for local
  dev. It is never set in production (the binary logs a warning at boot when
  it is).

## Unauthenticated surface

- **`GET /status`** is intentionally auth-free (service name, version, a
  data-dir health boolean and a project count — no content, no names of
  projects) so gatus can probe liveness. It is served on a Traefik monitor
  router that bypasses oauth2-proxy; everything else on the host stays gated.

## The filesystem is the attack surface

Every project/file name crosses the URL → filesystem boundary, so:

- **Allowlist validation before any path is built** (`backend/src/store.rs`):
  ASCII alphanumerics plus `. _ - space`, and any non-ASCII non-control
  character (ö/ä/å are valid names; everything path-dangerous — `/`, `\`,
  NUL, `..` — is ASCII and stays excluded). ≤128 bytes, no leading dot/space,
  no trailing dot/space. Separators and traversal never reach `Path::join`.
  Files must additionally end in `.md` — the store cannot be used to park
  arbitrary file types. Non-ASCII project names reach the bundle
  `Content-Disposition` only RFC 5987-encoded (`filename*`), never raw.
- **Writes never create structure implicitly**: PUT to a missing project is a
  404, not a `mkdir`.
- **Bundle download** zips only the validated `.md` files of one validated
  project; the `Content-Disposition` filename is the already-allowlisted
  project name.
- **Static file serving** resolves the requested path and rejects anything
  that escapes `STATIC_DIR` after canonicalisation (path-traversal guard);
  unmatched routes return the SPA shell, never an arbitrary file.

## Content (stored markdown is untrusted input)

- Markdown is rendered **client-side only** and every rendered block passes
  through **DOMPurify** before `{@html}` — script/iframe/event-handler payloads
  in an uploaded `.md` are stripped.
- **CSP** set in-code on every response: same-origin except Google Fonts;
  `img-src` additionally allows `https:` (scripts embed remote screenshots);
  no inline scripts (build-hashed bootstrap only), `frame-ancestors 'none'`,
  `object-src 'none'`. HSTS / X-Frame-Options / X-Content-Type-Options are
  Traefik's job.

## Hardening

- **Container**: non-root user, `scratch` base (no shell/userland), LAN-only
  (`../raspi/tasks/network_restrict.py`), small `MemoryMax`. The data dir is
  the only writable mount.
- **Fail closed**: the binary refuses to boot if the data dir can't be
  created/used.

## Reporting

Personal single-user project. Open an issue, or just fix it.
