# represent — repo overview

Markdown demo-script presenter — view `.md` files on a phone at the table,
run a multi-file demo with per-file timers, and patch the script (highlight /
strike / note) minutes before the talk. Sibling in eetu's homebrew family
([halo](../halo), [chat](../chat), [scribe](../scribe),
[raspi-dashboard](../raspi-dashboard)) — Rust(axum) + SvelteKit, halo-design.

## Layout

```
backend/         Rust axum 0.8 — SQLite store (profiles/projects/documents),
                 OIDC + session auth, CSP, zip bundles, serves the SPA
frontend/        Svelte 5 + SvelteKit (adapter-static), marked + DOMPurify
e2e/             spawned-binary integration tests (temp SQLite, real HTTP)
.claude/skills/  represent-design skill (glyph, wordmark, voice)
Dockerfile       multi-stage: node build + Rust cross-compile (arm64) → scratch
SECURITY.md      auth trust model, name allowlist, sanitize/CSP notes
```

Cargo workspace = `backend` + `e2e`.

## Conventions

- **Multi-user, three credential paths** (`auth::AuthUser`, in precedence
  order): own OIDC session cookie (`/auth/login` → kanidm, scribe-ported
  `oidc.rs`), oauth2-proxy forward-auth headers, `DEV_AUTH=1` synthetic dev
  identity (`/auth/login?username=alice` mints arbitrary dev sessions).
  Every identity resolves to a `profile` row (sub → email backfill →
  create); all queries key off `profile_id`. `/status` stays unauth.
- **SQLite is the database** (`REPRESENT_DB_PATH`, one file to back up/move):
  `profile → projects → documents` with content TEXT + `position` for demo
  order (fresh uploads get max+1). House Db wrapper: one
  `Arc<Mutex<Connection>>`, WAL, idempotent boot migrations. Names still
  pass the allowlist (`store::valid_name`) — they reach headers/zip entries.
  `REPRESENT_IMPORT_DIR`+`_EMAIL` one-shot-import the legacy flat-file
  layout at boot (existing projects skipped).
- **Markdown semantics live in the frontend** (`src/lib/markdown.ts`): the
  backend stores opaque text. Frontmatter is a minimal `key: value` fence —
  `timer: 90` or `timer: 1:30` (demo countdown), `title:`. `==text==` renders
  as `<mark>` via a marked extension; everything rendered goes through
  DOMPurify before `{@html}`.
- **Quick edits are source rewrites.** A DOM selection is mapped back to the
  markdown source via per-block `[start, end)` offsets from the lexer
  (`renderBlocks`), then the source is wrapped (`==`/`~~`) or a
  `> **note:**` block is inserted, and the whole file is PUT back. Removal is
  the same trip in reverse: tapping a rendered `mark`/`del` (or a note's `✕`)
  unwraps/deletes in the source (`unwrapArtifact`/`removeBlock`).
- **Offline is two layers** (`SECURITY.md`-irrelevant, demo-critical):
  `src/service-worker.ts` caches the app shell; `api.ts` mirrors every GET
  into localStorage, serves it when fetch fails (`netState.offline`), queues
  offline saves (latest-per-file) and flushes on reconnect. Viewer prefetches
  the whole project. `/api` is never cached by the SW — only by the api layer.
- **Type sharing is manual**: `frontend/src/lib/api.ts` mirrors
  `backend/src/{store,routes}.rs` structs by hand.

## Working on this repo

- Backend `:3008` (`REPRESENT_BIND`): `cd backend && cp .env.example .env`
  once, then `bacon` (default job runs + restarts on src/.env change) or
  `cargo run`. The .env sets `DEV_AUTH=1` + `REPRESENT_DB_PATH=../represent.db`
  (gitignored).
- Frontend dev `:5173`: `cd frontend && yarn install && yarn dev`; Vite
  proxies `/api` + `/status` to `:3008`. `yarn validate` = typecheck + lint +
  format.
- e2e: `cargo test -p represent-e2e -- --ignored` (builds the binary first:
  `cargo build -p represent-backend`).
- Key env: `REPRESENT_BIND`, `REPRESENT_DB_PATH`, `STATIC_DIR`, `DEV_AUTH`,
  `SESSION_KEY`, `OIDC_*`, `REPRESENT_IMPORT_DIR`/`_EMAIL`. See
  `backend/src/config.rs`.
- Hooks: `./install-hooks.sh` once after clone.

## Roadmap (v2 — don't build prematurely)

- Desktop WYSIWYG edit mode (the current `source` textarea is the v1 stopgap).
- File rename/reorder UI.

Out of scope: non-markdown file types, sharing projects between profiles.
If a feature crosses into those, raise it before implementing.
