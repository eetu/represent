# Security model — represent

A LAN-only, multi-user markdown viewer/editor for demo scripts. Durable state
is **one SQLite file** (`REPRESENT_DB_PATH`: profiles, projects, documents)
on a restic-backed bind mount. The only upstream it ever talks to is the
optional OIDC issuer.

## Trust boundaries & identity

Three credential paths, in extractor precedence (`backend/src/auth.rs`):

1. **Own OIDC session** (when `OIDC_*` is configured): authorization-code +
   PKCE against Kanidm, ported from scribe/chat. The browser never sees
   provider tokens — after ID-token validation only `sub|email` goes into the
   signed `represent_session` cookie (`SESSION_KEY` ≥64 bytes hex; prod
   refuses to boot without it; `http_only`, `SameSite=Lax`, `Secure` in
   prod). Handshake values (csrf/nonce/PKCE/next) round-trip in a separate
   10-minute signed cookie, deleted on first use; `next` is sanitized
   against open redirects (`/…` only, no `//host`).
2. **oauth2-proxy forward-auth headers** (`X-Auth-Request-User`/`-Email`) —
   the gated-host deploy mode; the app then needs no own login. Headers are
   PII and **never logged**.
3. **`DEV_AUTH=1`**: synthetic dev identity + `?username=` dev sessions for
   multi-user testing. Never set in production (boot warning).

Every identity resolves to a `profile` row and **all project/document queries
are keyed by profile id** — users cannot see or touch each other's data
(tested in `store::tests::profiles_are_isolated` + e2e).

## Unauthenticated surface

- **`GET /status`** is intentionally auth-free (service name, version, a
  data-dir health boolean and a project count — no content, no names of
  projects) so gatus can probe liveness. It is served on a Traefik monitor
  router that bypasses oauth2-proxy; everything else on the host stays gated.

## Input surface

- **All SQL is parameterized** (`rusqlite params!`); the store never
  interpolates user input into a query.
- **Name allowlist still applies** (`backend/src/store.rs`) even though
  names no longer touch the filesystem — they end up in zip entry names,
  `Content-Disposition` headers and URLs: ASCII alphanumerics plus
  `. _ - space`, and any non-ASCII non-control character (ö/ä/å valid;
  everything header/path-dangerous is ASCII and excluded). ≤128 bytes, no
  leading dot/space, no trailing dot/space. Files must end in `.md`. Writes
  to a missing project are 404, never implicit creation. Non-ASCII project
  names reach the bundle `Content-Disposition` only RFC 5987-encoded
  (`filename*`), never raw.
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
  (`../raspi/tasks/network_restrict.py`), small `MemoryMax`. The `/data`
  mount (one SQLite file) is the only writable path.
- **Fail closed**: the binary refuses to boot if the DB can't be opened, or
  if `SESSION_KEY` is missing/weak while `DEV_AUTH` is off.
- **OIDC HTTP client** disables redirects (SSRF guard per openidconnect-rs
  guidance) and uses short timeouts; discovery is lazy with single-flight
  retry, so a down issuer degrades login to a retryable 503, never a boot
  loop.

## Reporting

Personal single-user project. Open an issue, or just fix it.
