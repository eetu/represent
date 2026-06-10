# represent — repo overview

Markdown demo-script presenter — view `.md` files on a phone at the table,
run a multi-file demo with per-file timers, and patch the script (highlight /
strike / note) minutes before the talk. Sibling in eetu's homebrew family
([halo](../halo), [chat](../chat), [scribe](../scribe),
[raspi-dashboard](../raspi-dashboard)) — Rust(axum) + SvelteKit, halo-design.

## Layout

```
backend/         Rust axum 0.8 — flat-file project store (no DB), forward-auth
                 gate, CSP, zip bundles, serves the SPA
frontend/        Svelte 5 + SvelteKit (adapter-static), marked + DOMPurify
e2e/             spawned-binary integration tests (temp data dir, real HTTP)
.claude/skills/  represent-design skill (glyph, wordmark, voice)
Dockerfile       multi-stage: node build + Rust cross-compile (arm64) → scratch
SECURITY.md      forward-auth trust model, name allowlist, sanitize/CSP notes
```

Cargo workspace = `backend` + `e2e`.

## Conventions

- **Auth at the edge.** Behind oauth2-proxy (Traefik forward-auth). `/status`
  is unauthenticated (gatus liveness); all `/api/*` require
  `X-Auth-Request-User`/`-Email`. `DEV_AUTH=1` bypasses on localhost.
- **The filesystem is the database.** `REPRESENT_DATA_DIR/<project>/<file>.md`.
  Names pass a strict allowlist (`store::valid_name`) before any path is
  built; files must end `.md`. Demo order is metadata, not naming: a hidden
  `.order` JSON sidecar per project (`store::reorder`); unlisted files sort
  alphabetically after the ordered ones. Names are never rewritten.
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
  `cargo run`. The .env sets `DEV_AUTH=1` + `REPRESENT_DATA_DIR=../data`
  (gitignored).
- Frontend dev `:5173`: `cd frontend && yarn install && yarn dev`; Vite
  proxies `/api` + `/status` to `:3008`. `yarn validate` = typecheck + lint +
  format.
- e2e: `cargo test -p represent-e2e -- --ignored` (builds the binary first:
  `cargo build -p represent-backend`).
- Key env: `REPRESENT_BIND`, `REPRESENT_DATA_DIR`, `STATIC_DIR`, `DEV_AUTH`.
  See `backend/src/config.rs`.
- Hooks: `./install-hooks.sh` once after clone.

## Roadmap (v2 — don't build prematurely)

- Desktop WYSIWYG edit mode (the current `source` textarea is the v1 stopgap).
- File rename/reorder UI.

Out of scope: own auth, multi-user/per-user state, non-markdown file types.
If a feature crosses into those, raise it before implementing.
