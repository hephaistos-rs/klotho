---
name: Klotho
description: A self-hosted git forge. Every repository starts here, and every commit adds to the thread.
colors:
  # Light theme. Dark values are in the Colors section and the sidecar.
  linen: "oklch(0.985 0.003 255)"          # --background
  sheet: "oklch(1 0 0)"                    # --card, --popover
  iron-ink: "oklch(0.21 0.025 265)"        # --foreground
  wool-grey: "oklch(0.955 0.006 260)"      # --muted, --secondary
  slate-ink: "oklch(0.48 0.02 265)"        # --muted-foreground
  rule: "oklch(0.90 0.008 260)"            # --border
  rule-strong: "oklch(0.64 0.015 260)"     # --input (form-control boundary, 3:1)
  indigo-dye: "oklch(0.45 0.15 270)"       # --primary
  indigo-wash: "oklch(0.94 0.02 270)"      # --accent (hover and selected fill)
  focus-indigo: "oklch(0.55 0.16 270)"     # --ring
  madder: "oklch(0.52 0.19 27)"            # --destructive
  madder-wash: "oklch(0.955 0.02 25)"      # --destructive-soft
  madder-ink: "oklch(0.45 0.16 27)"        # --destructive-ink
  verdigris: "oklch(0.50 0.11 160)"        # --success
  verdigris-wash: "oklch(0.955 0.03 160)"  # --success-soft
  verdigris-ink: "oklch(0.40 0.09 160)"    # --success-ink
  amber: "oklch(0.62 0.13 58)"             # --warning (icons only)
  amber-wash: "oklch(0.96 0.025 75)"       # --warning-soft
  amber-ink: "oklch(0.47 0.10 55)"         # --warning-ink
  spindle: "oklch(0.25 0.06 270)"          # --shell (header ground)
  spindle-ink: "oklch(0.95 0.01 265)"      # --shell-foreground
  spindle-muted: "oklch(0.78 0.03 265)"    # --shell-muted
  weld-thread: "oklch(0.80 0.13 82)"       # --thread (on the shell, and in dark)
  weld-thread-ink: "oklch(0.60 0.12 70)"   # --thread-ink (the thread on the light ground)
typography:
  page-title:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.5rem"
    fontWeight: 650
    lineHeight: 1.25
    letterSpacing: "-0.01em"
  section-title:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.125rem"
    fontWeight: 650
    lineHeight: 1.4
  body:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 400
    lineHeight: 1.5
  ui:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 500
    lineHeight: 1.43
  meta:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 400
    lineHeight: 1.4
  ref:
    fontFamily: "Atkinson Hyperlegible Mono, ui-monospace, monospace"
    fontSize: "0.8125rem"
    fontWeight: 500
    lineHeight: 1.4
  code:
    fontFamily: "Atkinson Hyperlegible Mono, ui-monospace, monospace"
    fontSize: "0.8125rem"
    fontWeight: 400
    lineHeight: 1.6
rounded:
  sm: "0.125rem"
  md: "0.25rem"
  lg: "0.375rem"
spacing:
  hair: "4px"
  tight: "8px"
  group: "16px"
  block: "24px"
  section: "40px"
components:
  button-primary:
    backgroundColor: "{colors.indigo-dye}"
    textColor: "{colors.linen}"
    typography: "{typography.ui}"
    rounded: "{rounded.md}"
    height: "36px"
    padding: "0 16px"
  button-secondary:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.iron-ink}"
    typography: "{typography.ui}"
    rounded: "{rounded.md}"
    height: "36px"
    padding: "0 16px"
  button-destructive:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.madder-ink}"
    typography: "{typography.ui}"
    rounded: "{rounded.md}"
    height: "32px"
    padding: "0 12px"
  input:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.iron-ink}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    height: "36px"
    padding: "0 12px"
  ref-chip:
    backgroundColor: "{colors.wool-grey}"
    textColor: "{colors.iron-ink}"
    typography: "{typography.ref}"
    rounded: "{rounded.sm}"
    padding: "1px 6px"
  shell:
    backgroundColor: "{colors.spindle}"
    textColor: "{colors.spindle-ink}"
    height: "52px"
---

# Design System: Klotho

> **Status: accepted 2026-10-03, with amendments** (P2 limited and `aria-hidden`, P3 moved to amber, P4 with `--font-weight-semibold: 650`, P8 rejected). Built from the docs and the class inventory (`.impeccable/class-inventory.md`). Impeccable normally writes DESIGN.md after the build. This run writes it first because the brief asks for it, and the documenter pass at the end will bring it in line with what actually ships.

## Overview

**Mode: Operate.** People come to Klotho to read code and history and to manage access. The UI is a working forge first: standard navigation, standard controls, dense where it needs to be. The world (Klotho the spinner, history as a thread) gets four things and only these: **type, palette, density and one signature move.**

**What the repository already says.** One centred column, plain forms read top to bottom, a dark filled primary button, an outlined red destructive button, identifiers in monospace, 4px corners, 1px rules, no shadows. Quiet, honest and unthemed. This system keeps all of that and turns it into tokens.

**Where it extends [P1].** Klotho spins the thread of life, and a forge's content *is* threads: commits strung on branches, refs as knots. The palette comes from what thread was dyed with before synthetics: **indigo** (the working colour: actions, links, focus), **madder** (danger), **verdigris** (success), and **weld**, the yellow of the thread itself, kept for the thread and nothing else. Neutrals are cool, undyed-wool greys, *not* cream: cream plus terracotta is the AI default, and this palette avoids it on purpose.

**Scene.** A developer at a desk, often with a dark editor beside the browser. Sometimes on a phone in daylight, reviewing a change. Neither theme is the "real" one: the UI follows `prefers-color-scheme`, with a manual override (NFR-UI-005), and both themes meet WCAG AA.

**The signature move [P2]: the thread.** A single 2px weld-yellow line. Today it runs along the bottom edge of the indigo header (the spun thread under every page) and marks the current place in navigation. In Phase 3 it becomes the commit log's spine: commits are beads on one vertical thread, merges are where threads join. In Phase 8 it becomes the spun → measured → cut stages of the thread view (FR-UI-040). It's never a decorative stripe: it means *history* or *where you are in it*.

## Colors

Restrained strategy with a committed shell: tinted neutrals, one working accent (indigo), semantic dyes for state, a coloured header, and the thread. Colours are tokens in `crates/web/styles.css`, used through Tailwind classes. Markup never uses raw palette utilities (`gray-600`, `red-50`) or arbitrary values.

### Primary
- **Indigo dye** (`--primary`): primary buttons, links, the selected tab, checked controls. Light `oklch(0.45 0.15 270)` (7.4:1 on the ground), dark `oklch(0.75 0.12 272)` (8.5:1).
- **Indigo wash** (`--accent`): hover and selected fills on rows, menu items and ghost buttons. Light `oklch(0.94 0.02 270)`, dark `oklch(0.26 0.03 270)`.
- **Focus** (`--ring`): light `oklch(0.55 0.16 270)` (4.8:1), dark `oklch(0.68 0.13 272)` (6.6:1).

### Secondary: semantic dyes
- **Madder** (`--destructive`): revoke, delete, error. Solid `oklch(0.52 0.19 27)` light / `oklch(0.70 0.16 25)` dark. Wash for alert fills `oklch(0.955 0.02 25)` / `oklch(0.26 0.05 25)`. Ink on that wash `oklch(0.45 0.16 27)` / `oklch(0.82 0.09 25)` (7.0:1 / 8.8:1).
- **Verdigris** (`--success`): created, merged, passing. Solid `oklch(0.50 0.11 160)` / `oklch(0.74 0.12 160)`. Wash `oklch(0.955 0.03 160)` / `oklch(0.26 0.04 160)`. Ink `oklch(0.40 0.09 160)` / `oklch(0.82 0.10 160)`.
- **Warning** [P3, accepted as amended] (`--warning`): amber at hue 55–60, clearly apart from the weld thread (82). Solid `oklch(0.62 0.13 58)` light / `oklch(0.76 0.14 62)` dark, for icons only (3.6:1 / 8.7:1). Wash `oklch(0.96 0.025 75)` / `oklch(0.27 0.045 60)`. Ink `oklch(0.47 0.10 55)` / `oklch(0.84 0.10 65)` (6.3:1 / 9.2:1). **A warning is always an icon plus text**, never a line, stripe or border on its own.

### Tertiary: the thread
- **Weld thread** (`--thread`): `oklch(0.80 0.13 82)`. On the indigo shell (8.5:1 light, 10.7:1 dark) and on the dark ground (10.2:1).
- **Weld thread ink** (`--thread-ink`): `oklch(0.60 0.12 70)`. The thread on the light ground (3.9:1, enough for a non-text graphic).

### Neutral
| Token | Light | Dark | Use |
|---|---|---|---|
| `--background` | `oklch(0.985 0.003 255)` | `oklch(0.165 0.02 268)` | page ground |
| `--foreground` | `oklch(0.21 0.025 265)` | `oklch(0.95 0.008 260)` | ink (17:1 / 16.7:1) |
| `--card`, `--popover` | `oklch(1 0 0)` | `oklch(0.20 0.022 268)` | panels, lists, menus |
| `--muted`, `--secondary` | `oklch(0.955 0.006 260)` | `oklch(0.235 0.022 268)` | quiet fills, ref chips, table heads |
| `--muted-foreground` | `oklch(0.48 0.02 265)` | `oklch(0.72 0.02 265)` | secondary text (6.3:1 / 7.8:1) |
| `--border` | `oklch(0.90 0.008 260)` | `oklch(0.30 0.025 268)` | rules and dividers |
| `--input` | `oklch(0.64 0.015 260)` | `oklch(0.52 0.025 268)` | form-control boundary (3.2:1 / 3.5:1) |
| `--shell` | `oklch(0.25 0.06 270)` | `oklch(0.125 0.025 268)` | header ground, both themes |
| `--shell-foreground` | `oklch(0.95 0.01 265)` | `oklch(0.95 0.008 260)` | header text (13.9:1 / 17.5:1) |
| `--shell-muted` | `oklch(0.78 0.03 265)` | `oklch(0.74 0.025 265)` | header secondary text (8.1:1 / 8.8:1) |

### Named Rules
- **The thread is only the thread.** `--thread` / `--thread-ink` appear only on the thread line, its beads, and the current-location marker. Never on buttons, badges, headings or backgrounds.
- **Colour means state.** Indigo means "you can act" or "you are here". Madder, verdigris and weld mean what they say. Inactive things stay neutral.
- **Tokens only.** No `zinc-*`/`gray-*`/`red-*` palette utilities, no `[#…]` or `[oklch(…)]` arbitrary values in markup. A missing colour becomes a new token in `styles.css`.

## Typography

**One family with its mono sibling [P4]: Atkinson Hyperlegible Next and Atkinson Hyperlegible Mono**, self-hosted (SIL OFL). It was drawn by the Braille Institute so that easily confused characters stay distinct: `l 1 I`, `0 O`, `rn m`. That's the job a forge's type has to do: refs, hashes, paths and tokens are read character by character. The mono sibling shares its skeleton, so a hash inside a sentence doesn't jolt. Fallbacks are the system stacks, so a page without the font is still correct.

### Hierarchy
Fixed rem sizes, ratio about 1.2, no fluid type (Operate).

| Role | Size / weight / line height | Tailwind |
|---|---|---|
| Page title (`<h1>`) | 1.5rem / 650 / 1.25, tracking -0.01em | `text-2xl font-semibold tracking-tight` |
| Home title | 1.875rem / 650 | `text-3xl font-semibold tracking-tight` |
| Section title (`<h2>`) | 1.125rem / 650 / 1.4 | `text-lg font-semibold` |
| Body | 1rem / 400 / 1.5, prose up to 70ch | `text-base` |
| UI (labels, buttons, nav) | 0.875rem / 500 | `text-sm font-medium` |
| Meta (details, timestamps) | 0.8125rem / 400, `--muted-foreground` | `text-meta text-muted-foreground` (`--text-meta` is a theme size token) |
| Ref / hash / scope / secret | 0.8125rem mono / 500 | `font-mono text-meta` |
| Code (file view, diffs) | 0.8125rem mono / 400 / 1.6, `tabular-nums` | `font-mono text-meta leading-relaxed` |

### Named Rules
- **Monospace is for things you could type into git.** Refs, hashes, paths, scope names, tokens, clone URLs, code. Never for labels, headings or decoration.
- **Numbers line up.** Counts, sizes and dates in lists use `tabular-nums`.
- **Bold is rare.** 650 for titles (`font-semibold`, with `--font-weight-semibold: 650` set in `@theme` so the class and this spec agree), 500 for UI, 400 for everything else. No 700+.

## Layout

- **Shell:** a full-width indigo header, 52px tall, with the 2px thread along its bottom edge. Left: wordmark. Right: navigation and account. Below 640px the right side keeps only the account items, and further links move into a `<details>` menu, which works without JS [P5].
- **Frame [P6]:** content in `max-w-6xl` (72rem) with `px-4 sm:px-6`. That's wider than today's `max-w-3xl`, because the Phase 3 code browser needs the width. Forms and single-purpose pages (sign in, register) stay narrow: `max-w-md`, left-aligned with the page title, not centred cards.
- **Rhythm:** a 4px base. Inside a group 8px, between fields 16px, between blocks 24px. Above a section title 40px and below it 12px: more space above a heading than below it.
- **Lists over cards.** Collections (tokens, repositories, branches, commits) are bordered lists or tables with `divide-y`, not grids of cards.
- **Responsive is structural.** Rows wrap their actions below the text under 640px. Tables scroll inside their own container, never the page. Usable from 360px (NFR-UI-003).

## Elevation & Depth

Flat. Separation comes from 1px `--border` rules and the `--card` vs `--background` step. The only shadow is on things that float (menus, dialogs): `0 8px 24px -8px` in `--foreground` at 20% opacity. No shadows on buttons, cards or inputs.

## Shapes

- `--radius: 0.25rem` (the incumbent `rounded`). Small radius `0.125rem` for ref chips and checkboxes, large `0.375rem` for menus and dialogs. Nothing pill-shaped except a status dot.
- Lines are 1px. The thread is the only 2px line.

## Components

The vendored Topcoat UI components (`crates/web/src/components/`), themed by the tokens above. Every interactive component has default, hover, focus-visible, active, disabled and, where it applies, error states.

### Buttons
- **Primary:** `bg-primary text-primary-foreground`, 36px, `rounded-md`. Hover darkens via `bg-primary/90`. One per form.
- **Secondary / outline:** `bg-card border border-input text-foreground`, with `hover:bg-accent`.
- **Destructive:** outlined in the list context (`border-destructive/40 text-destructive-ink hover:bg-destructive-soft`), 32px. It's solid madder only inside a confirming dialog.
- **Ghost:** header and menu actions, `hover:bg-accent` (on the shell, `hover:bg-shell-foreground/10`).
- **Focus:** `focus-visible:ring-2 ring-ring ring-offset-2 ring-offset-background` on every control. Never remove the outline without replacing it.

### Chips
- **Ref chip:** `font-mono text-meta bg-muted rounded-sm px-1.5`, for branch, tag and scope names. A branch chip may carry a 6px dot in `--thread-ink` when the branch is the one shown.
- **Badges** for state (`Expired`, `Admin`): soft fills (`bg-success-soft text-success-ink`, `bg-destructive-soft text-destructive-ink`), sentence case.

### Cards / Containers
- A bordered list (`rounded-lg border bg-card divide-y`) is the default container. No nested cards. A notice that needs attention (a new token) is an **alert** with a soft fill and an icon, never a coloured left stripe.

### Inputs / Fields
- 36px, `bg-card border-input rounded-md px-3`, label above in `text-sm font-medium`, help text below in meta. Error: `border-destructive` plus a message in `text-destructive-ink` tied with `aria-describedby`.
- Checkboxes and selects are themed (`accent-color: var(--primary)` as the floor), never browser grey.
- Secrets and URLs to copy sit in a read-only mono field with a copy button (the button needs the runtime; the text stays selectable without it).

### Navigation
- Header links: `text-sm font-medium text-shell-muted hover:text-shell-foreground`. The current page gets `text-shell-foreground` plus a thread underline (the 2px weld line, `aria-current="page"`).
- Sign out is a ghost button inside a `<form method="post">`, styled like the links.

### Signature: the thread
- **Header thread:** `border-b-2 border-thread` on the shell. Decorative: it is a border, so nothing to announce.
- **Current-nav marker:** a 2px weld underline on the current link, drawn by CSS (`aria-hidden` if it is an element). The link itself carries `aria-current="page"`.
- **Nowhere else.** P2 is accepted for the header, the current-nav marker and the commit log only.
- **History thread [P2, Phase 3]:** decorative, so the line and the beads are `aria-hidden="true"`; the commit log is an `<ol>` with a 2px `--thread-ink` line down the gutter and an 8px bead per commit. A merge commit's bead is hollow. The text works without the line.
- **Theme toggle [P7]:** a three-way form (System / Light / Dark) that posts to `/-/theme` and sets a cookie. The server renders `class="dark"` on `<html>` when the cookie says dark. With no cookie, CSS follows `prefers-color-scheme`. The runtime only makes it instant.
- **Wordmark:** "Klotho" as text only, in Atkinson Hyperlegible Next 650 (P8, the spindle mark, was rejected for now).

### Icons [P9]
One set, Lucide (through Topcoat's Iconify support, embedded at build time), 16px in UI text and 1.5px stroke, `currentColor`. No emoji or Unicode glyphs as icons.

## Do's and Don'ts

### Do:
- Use theme tokens through Tailwind classes (`bg-primary`, `text-muted-foreground`, `border-input`).
- Keep every page working with JS off: real links, real `POST` forms, `<details>` for disclosure.
- Put refs, hashes, paths, scopes and secrets in mono. Put everything else in the sans.
- Check every new colour pair for AA in both themes before it ships.
- Write copy that names the action and the consequence ("Revoke", "It won't be shown again").

### Don't:
- Don't use raw palette utilities (`bg-gray-900`, `text-red-700`, `zinc-*`) or arbitrary colour values.
- Don't use the thread colour for anything but the thread.
- Don't use cream grounds, terracotta accents, gradient text, glassmorphism, or a neon-on-black "hacker" look.
- Don't use cards in grids for lists, nested cards, coloured left-border callouts, eyebrow labels above headings, or section numbers.
- Don't load fonts, icons or scripts from a third-party host. Everything comes from `/-/assets/`.
- Don't add motion beyond 150–200ms colour and opacity transitions on hover and focus. Honour `prefers-reduced-motion`.

## Proposals and decisions (2026-10-03)

| ID | Proposal | Decision |
|---|---|---|
| P1 | Natural-dye palette: indigo (action), madder (danger), verdigris (success), weld (the thread), cool wool-grey neutrals | Accepted |
| P2 | The thread as the one signature move: the header's 2px edge, the current-nav marker, and later the commit log spine and the thread view | Accepted for the header, the current-nav marker and the commit log only. Thread and beads are `aria-hidden` |
| P3 | Warning colour | Accepted, moved to amber (hue 55–60) so it cannot be mistaken for the thread. Always an icon plus text, never a line |
| P4 | Atkinson Hyperlegible Next and Mono, self-hosted | Accepted. `--font-weight-semibold: 650` in `@theme` |
| P5 | Mobile header folds extra links into a `<details>` menu | Accepted |
| P6 | Frame widens to `max-w-6xl`; forms stay `max-w-md` | Accepted. Long code lines scroll inside their own block, never the page (rubric R16) |
| P7 | Theme toggle as a cookie set by a `POST /-/theme` form (UI-only preference, no API endpoint, like account security) | Accepted (NFR-UI-005 asks for the override). The UI-only exception is recorded in ADR 0005 |
| P8 | Authored spindle mark next to the wordmark | Rejected for now: text wordmark only |
| P9 | Lucide as the one Iconify icon set | Accepted |
