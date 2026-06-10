# represent

**Markdown demo scripts — viewed, timed, and patched at the table.** Write the
script elsewhere, copy the `.md` files in, and run the demo from a phone:
swipe through the files in order, watch the per-file countdown, and when the
plan changes five minutes before the talk, highlight, strike through, or drop
a note straight into the script. Sibling in the homebrew family.

A single Rust (axum) binary that embeds a SvelteKit SPA, ships as one arm64
container to `ghcr.io/eetu/represent`, and is deployed onto the Pi by
[`../raspi`](../raspi) behind Traefik + oauth2-proxy. No database — projects
are directories of markdown files on a backed-up bind mount.

## How it works

- **Projects** are directories; **files** are plain `.md` with whatever names
  they arrived with. The running order is separate metadata (a `.order`
  sidecar per project) — drag rows in the project view to set it. Files not
  yet ordered (fresh uploads) follow alphabetically at the end.
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
backend/    Rust axum service — flat-file store, name allowlist, forward-auth
            gate, CSP, zip bundles, SPA serving. No DB.
frontend/   SvelteKit SPA (adapter-static, runes), marked + DOMPurify,
            halo-design tokens.
e2e/        Spawned-binary integration harness (temp data dir, real HTTP).
Dockerfile  Multi-stage xx cross-compile → scratch.
```

## Endpoints

| Route | Auth | Purpose |
|---|---|---|
| `GET /status` | none | Liveness: `{service, version, data_dir_healthy, project_count}` |
| `GET/POST /api/projects` | forward-auth | List / create projects |
| `DELETE /api/projects/{p}` | forward-auth | Delete a project |
| `GET /api/projects/{p}/files` | forward-auth | List files (demo order) |
| `GET/PUT/DELETE /api/projects/{p}/files/{f}` | forward-auth | Read / upsert / delete one file |
| `POST /api/projects/{p}/files/{f}/rename` | forward-auth | Rename a file (keeps its order slot) |
| `POST /api/projects/{p}/reorder` | forward-auth | Persist a new demo order (names untouched) |
| `GET /api/projects/{p}/bundle` | forward-auth | Zip download of the project |

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
