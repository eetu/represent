# represent

**Markdown demo scripts — viewed, timed, and patched at the table.** Write the
script elsewhere, copy the `.md` files in, and run the demo from a phone:
swipe through the files in order, watch the per-file countdown, and when the
plan changes five minutes before the talk, highlight, strike through, or drop
a note straight into the script. Sibling in the homebrew family.

A single Rust (axum) binary that embeds a SvelteKit SPA, ships as one arm64
container to `ghcr.io/eetu/represent`, and is deployed onto the Pi by
[`../raspi`](../raspi). Multi-user: identities come from an own OIDC login
(kanidm), oauth2-proxy forward-auth headers, or `DEV_AUTH` — each resolves to
a profile that owns its projects. All durable state is **one SQLite file**
(projects + documents as rows), so backup and moving the installation is
copying one file.

## How it works

- **Projects** own **documents** — plain markdown rows in SQLite, keeping
  whatever names they arrived with. The running order is a `position`
  column — drag rows in the project view to set it; fresh uploads land at
  the end. A legacy flat-file data dir imports once at boot via
  `REPRESENT_IMPORT_DIR` + `REPRESENT_IMPORT_EMAIL`.
- **Demo mode**: chrome collapses to a timer bar + position; swipe left/right
  (or ←/→) between files; the screen stays awake (wake lock). The timer comes
  from the file's own frontmatter and never auto-advances:

  ```markdown
  ---
  title: kickoff
  timer: 1:30
  ---

  # the new dashboard
  …
  ```

- **Quick edit** (read mode): select text → `mark` / `strike` / `note`.
  Highlights are written as `==text==`, strikes as `~~text~~`, notes as a
  `> **note:**` blockquote after the paragraph — plain markdown, so the files
  survive a round-trip through any other tool. Undo: notes carry a `✕`;
  tapping a highlight/strike pops a floating round `✕` above it (hover
  outlines it on desktop) — absolutely positioned, so the text never shifts.
- **Offline**: opening any file syncs the whole project into a local copy
  (localStorage) and a service worker caches the app shell — if the backend
  vanishes mid-talk, the demo keeps running on the local copy and edits are
  queued and flushed when it returns.
- **Source mode**: a raw textarea for structural last-minute surgery. (A real
  desktop WYSIWYG editor is v2.)
- **In/out**: `upload` PUTs local `.md` files into a project; `bundle`
  downloads the project as a zip with all edits applied.

## Layout

```
backend/    Rust axum service — SQLite store (profiles/projects/documents),
            OIDC + session auth, name allowlist, CSP, zip bundles, SPA serving.
frontend/   SvelteKit SPA (adapter-static, runes), marked + DOMPurify,
            halo-design tokens.
e2e/        Spawned-binary integration harness (temp SQLite, real HTTP).
Dockerfile  Multi-stage xx cross-compile → scratch.
```

## Endpoints

| Route | Auth | Purpose |
|---|---|---|
| `GET /status` | none | Liveness: `{service, version, db_healthy, project_count, oidc_healthy}` |
| `GET /auth/login`, `/auth/callback`, `POST /auth/logout` | none | OIDC (or DEV_AUTH) session flow |
| `GET /api/me` | session | `{email}` of the logged-in profile |
| `GET/POST /api/projects` | session | List / create the profile's projects |
| `DELETE /api/projects/{p}` | session | Delete a project |
| `GET /api/projects/{p}/files` | session | List files (demo order) |
| `GET/PUT/DELETE /api/projects/{p}/files/{f}` | session | Read / upsert / delete one file |
| `POST /api/projects/{p}/files/{f}/rename` | session | Rename a file (keeps its order slot) |
| `POST /api/projects/{p}/reorder` | session | Persist a new demo order (names untouched) |
| `GET /api/projects/{p}/bundle` | session | Zip download of the project |

"session" = any of: signed OIDC session cookie, oauth2-proxy forward-auth
headers, or the DEV_AUTH synthetic identity.

## Development

```sh
./install-hooks.sh                            # once
cd backend && cp .env.example .env && bacon   # backend on :3008 (DEV_AUTH=1,
                                              #   data in ../data), restarts on change
cd frontend && yarn install && yarn dev       # SPA on :5173, /api proxied
```

Tests: `cargo test --workspace` (unit) and
`cargo build -p represent-backend && cargo test -p represent-e2e -- --ignored`
(integration). Frontend: `yarn validate`.
