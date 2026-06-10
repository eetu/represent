---
name: represent-design
description: The per-app visual identity for represent — layers a glyph, wordmark, and voice on top of the shared halo-design tokens. Use when building or restyling any UI in this repo (viewer, demo chrome, lists, empty/error states). Inherits everything from halo-design; only the four deltas below are app-specific.
---

# represent-design

Thin layer over **halo-design** (the family tokens). Do not redefine colors or
type — import `src/lib/styles/halo.css` and consume `--halo-*` in scoped
`<style>` blocks. This skill owns only the four per-app deltas.

## 1. Glyph

A **script page with its active line lit** — a rounded document outline with
three text lines, and the warm dot (`--halo-accent`) as the **first character
of the active line** — fully inside the page outline, never touching the
border. Single-stroke `currentColor`, 1.8 stroke-width,
round caps/joins, 24×24 viewBox. Canonical implementation:
`src/lib/components/Wordmark.svelte`; icon sources:
`static/favicon.svg` + `static/icon-maskable.svg` (regenerate PNGs with
`scripts/gen-icons.sh`).

## 2. Wordmark

Text: **`represent`** (one word, lowercase) — `re` in `--halo-text-main`,
`present` in `--halo-accent` (the `.accent` span; presenting is the product).
Font: `--halo-font-heading` (Space Grotesk). The glyph sits to its left with a
`0.5em` gap.

## 3. Layout

`max-width: 720px`, centered — a phone-first reading column, not a dashboard.
No persistent shell: `+layout.svelte` is chrome-less and each route owns its
header, because the viewer must go (nearly) full-screen in demo mode.

- **Projects** (`/`) — wordmark + tagline, stacked `.halo-card` rows.
- **Project** (`/p/[project]`) — back arrow, name, `upload`/`bundle` actions,
  numbered file rows (the swipe order).
- **Viewer** (`/p/[project]/f/[file]`) — read mode: slim header + rendered
  markdown; demo mode: only the timer bar, `i / n` position and `exit`.

Reading text is the hero: body 1.05rem (1.25rem in demo), generous 1.65 line
height. The three edit artifacts have fixed looks — `==highlight==` →
`<mark>` on `--halo-accent-bg`, `~~strike~~` → muted `<del>`, notes →
accent-edged blockquote. Buttons are quiet bordered pills; the primary action
(`demo`, `save`, `add`) borrows the accent for border + text only — never a
filled block.

## 4. Voice

**No tagline** — the header is the wordmark alone (eetu's apps drop the
family's tagline convention from here on). Empty states stay one quiet, useful
line ("empty — upload files or add one below. name files 01-…, 02-… to set the
demo order."). No exclamation marks, no emoji; the timer's numbers do the
talking.
