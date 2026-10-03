---
name: Klotho
description: A self-hosted git forge. Every repository starts here, and every commit adds to the thread.
colors:
  # Light theme ("lamplight"). Dark ("night") values are in the Colors section.
  linen: "oklch(0.957 0.008 80)"           # --background
  sheet: "oklch(1 0 0)"                    # --card, --popover
  lamp-white: "oklch(0.99 0.004 80)"       # --primary-foreground, --destructive-foreground
  ink: "oklch(0.21 0.01 60)"               # --foreground
  flax: "oklch(0.935 0.01 80)"             # --muted, --secondary
  ash: "oklch(0.49 0.02 65)"               # --muted-foreground
  rule: "oklch(0.90 0.014 80)"             # --border
  rule-strong: "oklch(0.62 0.025 75)"      # --input (form-control boundary, 3:1)
  bronze: "oklch(0.50 0.10 70)"            # --primary (the gold, darkened for a light ground)
  gold-wash: "oklch(0.94 0.035 85)"        # --accent (hover and selected fill)
  focus-bronze: "oklch(0.55 0.105 72)"     # --ring
  madder: "oklch(0.50 0.16 28)"            # --destructive
  madder-wash: "oklch(0.955 0.02 30)"      # --destructive-soft
  madder-ink: "oklch(0.45 0.15 28)"        # --destructive-ink
  olive: "oklch(0.47 0.08 155)"            # --success
  olive-wash: "oklch(0.955 0.025 150)"     # --success-soft
  olive-ink: "oklch(0.40 0.07 155)"        # --success-ink
  amber: "oklch(0.62 0.13 65)"             # --warning (icons only)
  amber-wash: "oklch(0.96 0.03 85)"        # --warning-soft
  amber-ink: "oklch(0.47 0.10 60)"         # --warning-ink
  night: "oklch(0.17 0.02 295)"            # --shell (the header, night in both themes)
  night-linen: "oklch(0.93 0.017 80)"      # --shell-foreground
  night-muted: "oklch(0.72 0.02 300)"      # --shell-muted
  spun-gold: "oklch(0.76 0.13 78)"         # --thread (on the shell; 0.78 0.13 80 in dark)
  spun-gold-ink: "oklch(0.58 0.12 72)"     # --thread-ink (the thread on the light ground)
typography:
  page-title:
    fontFamily: "Cormorant Garamond, ui-serif, Georgia, serif"
    fontSize: "2.25rem"
    fontWeight: 650
    lineHeight: 1.25
  wordmark:
    fontFamily: "Cormorant Garamond, ui-serif, Georgia, serif"
    fontSize: "1.5rem"
    fontWeight: 650
    lineHeight: 1
  section-title:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.125rem"
    fontWeight: 650
    lineHeight: 1.556
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
    lineHeight: 1.4
rounded:
  sm: "0.125rem"
  md: "0.25rem"
  lg: "0.375rem"
spacing:
  hair: "4px"
  tight: "8px"
  row: "12px"
  group: "16px"
  block: "24px"
  section: "40px"
components:
  button-primary:
    backgroundColor: "{colors.bronze}"
    textColor: "{colors.lamp-white}"
    typography: "{typography.ui}"
    rounded: "{rounded.md}"
    height: "36px"
    padding: "0 16px"
  button-secondary:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.ink}"
    typography: "{typography.ui}"
    rounded: "{rounded.md}"
    height: "36px"
    padding: "0 16px"
  button-ghost:
    textColor: "{colors.ink}"
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
  button-destructive-confirm:
    backgroundColor: "{colors.madder}"
    textColor: "{colors.lamp-white}"
    typography: "{typography.ui}"
    rounded: "{rounded.md}"
    height: "32px"
    padding: "0 12px"
  button-shell:
    textColor: "{colors.night-muted}"
    typography: "{typography.ui}"
    rounded: "{rounded.md}"
    height: "32px"
    padding: "0 12px"
  input:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    height: "36px"
    padding: "0 12px"
  ref-chip:
    backgroundColor: "{colors.flax}"
    textColor: "{colors.ink}"
    typography: "{typography.ref}"
    rounded: "{rounded.sm}"
    padding: "1px 6px"
  badge-success:
    backgroundColor: "{colors.olive-wash}"
    textColor: "{colors.olive-ink}"
    typography: "{typography.meta}"
    rounded: "{rounded.md}"
    padding: "2px 8px"
  code-line:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.ink}"
    typography: "{typography.code}"
    rounded: "{rounded.md}"
    padding: "8px 12px"
  shell:
    backgroundColor: "{colors.night}"
    textColor: "{colors.night-linen}"
    height: "52px"
---

# Design System: Klotho

> **Status: records what shipped on 2026-10-03** (the sign-in, registration, access-token, home and 404 pages). The system was accepted the same day with amendments (P2 limited and `aria-hidden`, P3 moved to amber, P4 with `--font-weight-semibold: 650`, P8 rejected), written ahead of the build, and has now been brought in line with the code: `crates/web/styles.css`, `crates/web/src/components/`, `crates/web/src/ui.rs` and the pages. Where this file and the code disagree, the code is what ships; fix whichever is wrong in the same change. **Retuned the same day to Nyx** (palette and display type; chosen from the "Klotho redesign" design canvas): the earlier indigo, verdigris and weld palette is replaced, and page titles and the wordmark moved to Cormorant Garamond. Layout, components and rules are unchanged.

## Overview

**Mode: Operate.** People come to Klotho to read code and history and to manage access. The UI is a working forge first: standard navigation, standard controls, dense where it needs to be. The world (Klotho the spinner, history as a thread) gets four things and only these: **type, palette, density and one signature move.**

**What the repository already says.** One centred column, plain forms read top to bottom, a dark filled primary button, an outlined red destructive button, identifiers in monospace, 4px corners, 1px rules, no shadows. Quiet, honest and unthemed. This system keeps all of that and turns it into tokens.

**Where it extends [P1]: Nyx.** Klotho spins the thread of life, and a forge's content *is* threads: commits strung on branches, refs as knots. The Fates are daughters of Night, so Klotho spins by lamplight: the header is **night** in both themes, with a **spun-gold** thread along its edge. At night the whole page is night, linen text, and gold for everything that acts. By lamplight (the light theme) the page is linen and ink, and the gold darkens to **bronze** so it reads on a light ground. **Madder** is danger, **olive** is success. Neutrals are warm and nearly unsaturated, never purple-tinted in the light theme and never cream-plus-terracotta. The goddess is in the colour, the gold thread and the display serif, never in costume (no Greek keys, columns or laurels).

**Scene.** A developer at a desk, often with a dark editor beside the browser. Sometimes on a phone in daylight, reviewing a change. Neither theme is the "real" one: the UI follows `prefers-color-scheme`, with a manual override (NFR-UI-005), and both themes meet WCAG AA.

**The signature move [P2]: the thread.** A single 2px spun-gold line. Today it runs along the bottom edge of the night header (the spun thread under every page), comes off the top of the spindle beside the wordmark, and marks the current place in navigation. In Phase 3 it becomes the commit log's spine: commits are beads on one vertical thread, merges are where threads join. In Phase 8 it becomes the spun → measured → cut stages of the thread view (FR-UI-040). It's never a decorative stripe: it means *history* or *where you are in it*.

## Colors

Restrained strategy with a committed shell: warm neutrals, one working accent (bronze by lamplight, gold at night), semantic dyes for state, a night header, and the thread. Colours are tokens in `crates/web/styles.css`, used through Tailwind classes. Markup never uses raw palette utilities (`gray-600`, `red-50`) or arbitrary values. Contrast figures below are WCAG ratios measured from the OKLCH values (through OKLab and linear sRGB); "on the ground" means on `--background`.

### Primary
- **Bronze / spun gold** (`--primary`): primary buttons, links, checked controls, the current item in a menu. Light `oklch(0.50 0.10 70)`, bronze (5.4:1 on the ground, 6.1:1 on the card); dark `oklch(0.76 0.13 78)`, gold (8.8:1 on the ground, 8.3:1 on the card). Its foreground is `oklch(0.99 0.004 80)` in light (6.0:1) and `oklch(0.19 0.025 75)` in dark (8.5:1).
- **Gold wash** (`--accent`): hover and selected fills on menu items, ghost and outline buttons, and the chosen theme. Light `oklch(0.94 0.035 85)`, dark `oklch(0.27 0.035 295)`. Primary text on it: 5.1:1 / 7.0:1. Buttons press to `primary/15`.
- **Focus** (`--ring`): light `oklch(0.55 0.105 72)` (4.4:1 on the ground, 3.9:1 on the night header), dark `oklch(0.80 0.12 80)` (10.2:1).

### Secondary: semantic dyes
- **Madder** (`--destructive`): revoke, delete, error. Solid `oklch(0.50 0.16 28)` light / `oklch(0.72 0.13 28)` dark, with its foreground at 6.3:1 / 7.1:1. Wash for alert fills `oklch(0.955 0.02 30)` / `oklch(0.27 0.05 25)`. Ink on that wash `oklch(0.45 0.15 28)` / `oklch(0.83 0.08 28)` (7.0:1 / 8.8:1).
- **Olive** (`--success`): created, merged, passing. Solid `oklch(0.47 0.08 155)` / `oklch(0.79 0.10 155)`, used today only at 30% as the success alert's border. Wash `oklch(0.955 0.025 150)` / `oklch(0.26 0.04 155)`. Ink `oklch(0.40 0.07 155)` / `oklch(0.85 0.08 155)` (7.9:1 / 10.0:1 on the wash).
- **Warning** [P3, accepted as amended] (`--warning`): amber at hue 60–65, apart from the bronze and the gold thread (70–80) by lightness and by always carrying an icon and words. Solid `oklch(0.62 0.13 65)` light / `oklch(0.76 0.14 62)` dark, for icons only (3.3:1 / 8.6:1). Wash `oklch(0.96 0.03 85)` / `oklch(0.27 0.045 60)`. Ink `oklch(0.47 0.10 60)` / `oklch(0.84 0.10 65)` (6.3:1 / 9.2:1). **A warning is always an icon plus text**, never a line, stripe or border on its own. The tokens ship and the Warning badge uses them; no page shows a warning yet.

### Tertiary: the thread
- **Spun gold** (`--thread`): `oklch(0.76 0.13 78)` light, `oklch(0.78 0.13 80)` dark. On the night header (8.8:1 light, 9.9:1 dark) and on the dark ground (9.5:1).
- **Spun-gold ink** (`--thread-ink`): `oklch(0.58 0.12 72)`. The thread on the light ground (3.9:1, enough for a non-text graphic). In dark it is the same as `--thread`.

### Neutral
| Token | Light (lamplight) | Dark (night) | Use |
|---|---|---|---|
| `--background` | `oklch(0.957 0.008 80)` linen | `oklch(0.17 0.02 295)` night | page ground |
| `--foreground` | `oklch(0.21 0.01 60)` | `oklch(0.93 0.017 80)` | ink / linen text (15.7:1 / 15.6:1) |
| `--card`, `--popover` | `oklch(1 0 0)` | `oklch(0.205 0.024 295)` | lists, empty states, alerts, inputs, code lines, menus |
| `--muted`, `--secondary` | `oklch(0.935 0.01 80)` | `oklch(0.24 0.03 295)` | quiet fills: ref chips, the Secondary badge, read-only and disabled inputs |
| `--muted-foreground` | `oklch(0.49 0.02 65)` | `oklch(0.72 0.03 300)` | secondary text (5.6:1 / 7.7:1 on the ground, 6.3:1 / 7.2:1 on the card) |
| `--border` | `oklch(0.90 0.014 80)` | `oklch(0.31 0.035 295)` | rules and dividers |
| `--input` | `oklch(0.62 0.025 75)` | `oklch(0.52 0.035 300)` | form-control boundary (3.7:1 / 3.2:1 on the card) |
| `--shell` | `oklch(0.17 0.02 295)` | `oklch(0.13 0.015 295)` | header ground: night in both themes, a step below the dark ground |
| `--shell-foreground` | `oklch(0.93 0.017 80)` | `oklch(0.93 0.017 80)` | header text (15.6:1 / 16.4:1) |
| `--shell-muted` | `oklch(0.72 0.02 300)` | `oklch(0.72 0.03 300)` | header links at rest, the signed-in name (7.7:1 / 8.1:1) |

The violet in the night hues is deliberate and only appears in dark surfaces; the light theme's neutrals are warm (hue 60–80).

The floating shadow is a token too (`--shadow-float`, see Elevation & Depth). Text selection is `primary/25`.

### Named Rules
- **The thread is only the thread.** `--thread` / `--thread-ink` appear only on the thread line, its beads, the spindle mark and the current-location marker. Never on buttons, badges, headings or backgrounds.
- **Colour means state.** Bronze or gold means "you can act" or "you are here". Madder, olive and the thread mean what they say. Inactive things stay neutral.
- **Tokens only.** No `zinc-*`/`gray-*`/`red-*` palette utilities, no `[#…]` or `[oklch(…)]` arbitrary values in markup. A missing colour becomes a new token in `styles.css`, written in the light block and in both dark blocks (`:root.dark` and the `prefers-color-scheme` block), which stay identical.

## Typography

**One family with its mono sibling [P4]: Atkinson Hyperlegible Next and Atkinson Hyperlegible Mono**, self-hosted (SIL OFL). It was drawn by the Braille Institute so that easily confused characters stay distinct: `l 1 I`, `0 O`, `rn m`. That's the job a forge's type has to do: refs, hashes, paths and tokens are read character by character. The mono sibling shares its skeleton, so a hash inside a sentence doesn't jolt. Fallbacks are the system stacks, so a page without the font is still correct.

**A display serif for titles only: Cormorant Garamond** (SIL OFL), a Garamond drawn for large sizes, with the calligraphic contrast of an inscription. It sets the page `<h1>` and the wordmark and nothing else: no buttons, labels, section titles or body text, where legibility is Atkinson's job. Its small x-height is why titles are a step larger than a sans title would be. The fallback is the system serif stack.

The files are Fontsource's variable WOFF2 (`latin` and `latin-ext` subsets, `font-display: swap`; weights 200–800 for Atkinson, 300–700 for Cormorant), vendored in `crates/web/assets/fonts` and embedded in the release binary. Only the Latin sans file is preloaded.

### Hierarchy
Fixed rem sizes, ratio about 1.2, no fluid type (Operate).

| Role | Size / weight / line height | Tailwind |
|---|---|---|
| Page title (`<h1>`, `page_header`) | Cormorant Garamond, 1.875rem below 640px, 2.25rem from 640px / 650 / 1.25 | `font-display text-3xl sm:text-4xl leading-tight font-semibold` |
| Home title | the same `page_header` as every page | as above |
| Wordmark | Cormorant Garamond 1.5rem / 650 / 1, after the 24px spindle mark | `font-display text-2xl leading-none font-semibold` |
| Section title (`<h2>`, `section_heading`) | 1.125rem / 650 / 1.556 | `text-lg font-semibold` |
| Body | 1rem / 400 / 1.5; page descriptions up to 65ch | `text-base`, `max-w-prose` |
| UI (labels, buttons, nav, alerts) | 0.875rem / 500 / 1.43 (alert and help text at 400) | `text-sm font-medium` |
| Input text | 1rem below 640px (no zoom on phones), 0.875rem from 640px | `text-base sm:text-sm` |
| Meta (details, dates, hints, field errors at 500) | 0.8125rem / 400 / 1.4, `--muted-foreground` | `text-meta text-muted-foreground` (`--text-meta` is a theme size token) |
| Ref / hash / scope / secret | 0.8125rem mono / 500 | `font-mono text-meta font-medium` |
| Code line (`code_line`) | 0.8125rem mono / 400 / 1.4 | `font-mono text-meta` |

File views and diffs (Phase 3) aren't built yet; when they are, they use the code role with `leading-relaxed` and `tabular-nums`.

### Named Rules
- **Monospace is for things you could type into git.** Refs, hashes, paths, scope names, tokens, clone URLs, code. Never for labels, headings or decoration. Dates are in the sans with `tabular-nums`.
- **Numbers line up.** `<time>` and `<table>` get `tabular-nums` from the base layer; counts and sizes in lists use it too.
- **Bold is rare.** 650 for titles and the wordmark (`font-semibold`, with `--font-weight-semibold: 650` set in `@theme` so the class and this spec agree), 500 for UI, 400 for everything else. No 700+.

## Layout

- **Shell:** a full-width night header in both themes, 52px tall (`h-13`), with the 2px spun-gold thread along its bottom edge, and a footer under a 1px rule holding the theme switch. Header left: the spindle mark (an inline, `aria-hidden` SVG: shaft and whorl in the header text colour, the thread coming off the top in `--thread`) and the wordmark. Header right, signed in, from 640px: the navigation links, then the signed-in name (meta size, `--shell-muted`, set off by a 1px rule at `shell-foreground/20`, cut at 160px, prefixed "Signed in as" for screen readers), then **Sign out**. Below 640px all of that folds into one **Menu** button (Lucide `menu` icon plus the word) that opens a `<details>` menu aligned to the right edge: "Signed in as …" as its label, the links, a rule, and Sign out [P5]. Signed out, **Sign in** and **Register** stay in the header at every width. A "Skip to content" link appears on focus, styled as a floating panel.
- **Frame [P6]:** content in `max-w-6xl` (72rem) with `px-4 sm:px-6` and 32px above and below (`py-8`). Settings pages sit in a `max-w-3xl` column; the forms inside them are `max-w-md`. Single-purpose pages (sign in, register) are a `max-w-md` column, left-aligned with the page title, not centred cards.
- **Rhythm:** a 4px base. Inside a field (label, control, hint, error) 6px. Inside a group 8px. Between fields 16px: every form is a `flex flex-col gap-4` stack. `form_actions` and a `field_set` that follows a field get 24px above them; buttons in an action row sit 12px apart. The page header has 24px below it, its description 4px below the title. Above a section title 40px and below it 12px: more space above a heading than below it. A closing line under a form ("No account yet?") sits 24px below.
- **Lists over cards.** Collections (tokens, later repositories, branches, commits) are one bordered list (`list`, `list_row`) with hairline dividers, rows padded 12px × 16px, not grids of cards. An empty collection shows `empty_state`: the same bordered card, 24px tall padding, with what's missing in 500 and the next step in muted `text-sm`.
- **Responsive is structural.** From 640px a row's actions sit to the right of its content; below it they wrap underneath, left-aligned at their natural width (they don't stretch). Page and section actions wrap the same way. Long names wrap anywhere (`wrap-anywhere`) instead of widening the page. Tap targets grow to 40px below 640px (header buttons, the Menu trigger, menu items, the theme buttons). Usable from 360px (NFR-UI-003). Wide content, when tables arrive, scrolls inside its own container, never the page.

## Elevation & Depth

Flat. Separation comes from 1px `--border` rules and the `--card` vs `--background` step. Only things that float cast a shadow: the dropdown menu panel, the styled select picker and the skip link. The one shadow is `--shadow-float`, `0 8px 24px -8px` in `--foreground` at 20% in light and in black at 50% in dark; Tailwind's `shadow-sm`, `shadow-md` and `shadow-lg` all map to it, and `shadow-xs` to none. No shadows on buttons, lists, alerts or inputs.

## Shapes

- `--radius-md: 0.25rem` (the incumbent `rounded`) for buttons, inputs, selects, badges, menu items and code lines. Small `0.125rem` for ref chips, checkboxes and bare links' focus outline. Large `0.375rem` for lists, empty states, alerts, menus and the select picker (`xl` and `2xl` are capped at the same value). Nothing is pill-shaped.
- Lines are 1px. The thread is the only 2px line.

## Components

The vendored Topcoat UI components (`crates/web/src/components/`), themed by the tokens above, plus the shared pieces in `crates/web/src/ui.rs` that compose them. Interactive components have default, hover, focus-visible, active and disabled states, and error states where they apply.

**What ships:** alert, badge, button, checkbox, dropdown menu, field, input, label and select. **Not in the system yet:** a warning alert, larger buttons (`lg`, icon-only), tables, tabs, breadcrumbs, cards and dialogs. Each is added with `topcoat ui add` when a page first needs it, then rethemed to the rules in this file (tokens only, these radii, flat unless it floats, the focus ring below) in the same change.

### Buttons
- **Shape and size:** `rounded-md`, a 1px border on every variant (transparent where it isn't drawn), `text-sm font-medium`, 16px icons. Two sizes: **Md** 36px tall with 16px sides (the default, for forms), **Sm** 32px with 12px sides (list actions, the header, the theme switch, confirmations).
- **Primary:** `bg-primary text-primary-foreground`, hover `primary/90`, pressed `primary/80`. One per form.
- **Secondary:** `bg-card border-input text-foreground`, hover `bg-accent`.
- **Outline:** like Secondary on a transparent ground. Used for Cancel.
- **Ghost:** no fill until hovered (`bg-accent`).
- **Destructive outline:** `bg-card border-destructive/40 text-destructive-ink`, hover `bg-destructive-soft` with a stronger border, Sm. The first step of a destructive action in a list ("Revoke").
- **Destructive (solid madder):** only for the confirming button inside an inline confirmation ("Revoke token").
- **Shell:** the ghost button on the night header: `text-shell-muted`, hover `shell-foreground/10` fill and `text-shell-foreground`; `aria-current="page"` keeps it at full ink.
- **Disabled:** 50% opacity, no pointer events. Links styled as buttons (`button_variants`) get the same classes.
- **Focus:** every control draws its own ring and hides the outline: `focus-visible:outline-hidden` plus `ring-2 ring-ring ring-offset-2 ring-offset-background`. `outline-hidden` (not `outline-none`) keeps an outline in forced-colours mode. Menu items and select options inset the ring instead. Anything that doesn't draw a ring (bare links, `<summary>`, a scrolling code line) gets the base 2px `--ring` outline, offset 2px. Never remove the outline without replacing it.

### Chips
- **Ref chip** (`badge`, `Ref` variant): `font-mono text-meta font-medium`, `bg-muted` with a 1px `--border`, `rounded-sm`, 1px × 6px padding, for branch, tag and scope names. A long name is cut with an ellipsis instead of widening the row, and the full name goes in `title`. Where the chips repeat information already given as text (a token's scopes), the chip row is `aria-hidden` and an `sr-only` line carries the list.
- **Badges** for state: `text-meta font-medium`, `rounded-md`, 2px × 8px, soft fills, sentence case. **Success** (`bg-success-soft text-success-ink`: "New"), **Destructive** (`bg-destructive-soft text-destructive-ink`: "Expired"), **Secondary** (grey, for neutral statuses such as "Admin"), **Warning** (`bg-warning-soft text-warning-ink` with a 14px Lucide `triangle-alert`, so it's never colour alone).

### Cards / Containers
- A bordered list (`rounded-lg border bg-card divide-y`) is the default container. No nested cards, and no card component.
- **Alerts:** `rounded-lg`, 1px border, 12px × 16px padding, `text-sm`; a 16px icon column, a title in 500 and a description beneath. **Success** (olive wash, `success/30` border, `circle-check`), **Destructive** (madder wash, `destructive/30` border, `circle-alert`), **Neutral** (card fill, `--border`, muted description, icon optional). Never a coloured side stripe.
- **Announcing:** alerts carry no role of their own. Messages a page renders for the reader to notice are success notices with `role="status"` (`notice`: "Token “laptop” created", "Token revoked."); a form-level error is a Destructive alert with `role="alert"` (`form_error`), as is a field error.
- **Code line** (`code_line`): a value to read or copy character by character, in a block on the card colour with a 1px `--border`, `rounded-md`, 8px × 12px, mono meta. It reads on any surface, including inside an alert. It scrolls sideways on its own (and is focusable so the keyboard can scroll it), or with `wrap` breaks anywhere so a secret is fully visible. The new-token secret is a wrapping code line with `select-all`, so one click selects it. There's no copy button yet.

### Inputs / Fields
- **Input and select:** 36px, `bg-card border-input rounded-md`, 12px sides (the select keeps 32px on the right for its Lucide `chevron-down`, which turns over while the picker is open). Hover darkens the border to `--muted-foreground`. Focus turns the border `--ring` and adds the ring. Invalid (`aria-invalid="true"`): `border-destructive` and a madder ring. Read-only and disabled: `bg-muted`, disabled also at 50%. In browsers that support `appearance: base-select`, the select's picker is a themed floating panel with gold-wash hover and a checkmark on the chosen option; elsewhere it's the native picker.
- **Field:** label above in `text-sm font-medium`, then the control, then help text in meta and, when there is one, an error in meta 500 `text-destructive-ink` with `role="alert"`, both tied to the control with `aria-describedby`. An invalid field colours its label madder too. `ui::field` builds this for a plain required input.
- **Checkbox:** a custom-drawn 16px box (`rounded-sm`, `border-input`, `bg-card`) centred in a 24px target, filled with the action colour (bronze or gold) with a 14px Lucide check when checked. `accent-color: var(--primary)` on the root is the floor for any native control left unstyled; nothing is browser grey.
- **Groups:** a `field_set` with a `field_legend` (the **Label** size, `text-sm font-medium`, 12px above the first control) holds related checkboxes, each with its mono name and a meta description beside it, 16px apart.
- **Form actions:** `form_actions` is the submit row: the primary button first, then any secondary action, 12px apart, 24px below the last field.

### Navigation
- **Header links:** Shell buttons, Sm. The current page gets `aria-current="page"`, full ink and the thread underline.
- **Sign out** is a Shell button inside a `<form method="post" action="/-/logout">`, styled like the links. Below 640px it is the last item of the Menu, under a rule, away from the trigger.
- **Dropdown menu:** a native `<details>`, so it opens without JS. The panel is `bg-popover`, 1px `--border`, `rounded-lg`, 4px padding, at least 192px wide, with `--shadow-float`. Items are `text-sm`, 6px × 8px, `rounded-md`, hover `bg-accent`; the current page's item takes the gold wash, the action colour and 500. A label in meta 500 muted heads the list; a hairline separates groups. Actions that change state are `<button>`s inside a `POST` form.
- **Text links:** one style for every bare `<a>` (no classes) in running text: the action colour (bronze by lamplight, gold at night), 500, a 1px underline at 60% offset 4px that becomes a full-strength 2px line on hover, 150ms. Links with their own classes (navigation, buttons, rows) style themselves.

### Destructive actions: confirm inline
Revoking works without JS and without dialogs. The row's **Revoke** (Destructive outline, Sm) is a link to `?revoke=<id>#<row>`. That page renders the same row with the Revoke link gone and a Destructive alert inside it: "Revoke “name”?", the consequence ("Anything using it stops working. This can't be undone."), and a `form_actions` row with **Revoke token** (solid Destructive, Sm, a `POST` form) and **Cancel** (Outline, Sm, a link back to the row). A successful revoke redirects to `?revoked=1`, which shows the "Token revoked." success notice. An unknown or foreign id shows the plain page and claims nothing.

### Signature: the thread
- **Header thread:** `border-b-2 border-thread` on the shell. Decorative: it is a border, so nothing to announce.
- **Current-nav marker:** a 2px spun-gold underline on the current header link, drawn as a text decoration (`decoration-thread decoration-2 underline-offset-8`), so there's no element to hide. The link itself carries `aria-current="page"`. Inside the mobile menu the current item uses the gold wash instead.
- **Nowhere else.** P2 is accepted for the header, the current-nav marker and the commit log only.
- **History thread [P2, Phase 3, not built yet]:** decorative, so the line and the beads are `aria-hidden="true"`; the commit log is an `<ol>` with a 2px `--thread-ink` line down the gutter and an 8px bead per commit. A merge commit's bead is hollow. The text works without the line.
- **Theme switch [P7]:** in the footer, "Theme" in meta, then three submit buttons in one form (System, Light, Dark, each with its Lucide icon: `monitor`, `sun`, `moon`), Sm, 40px tall below 640px. The current choice is a Secondary button with the gold wash and `aria-pressed="true"`; the others are Ghost in muted text that darkens on hover. It posts to `/-/theme` with the page to return to, and stores `klotho_theme` (HTTP-only, `SameSite=Lax`, a year; choosing System deletes it). The server then renders `class="light"` or `class="dark"` on `<html>` and a matching `color-scheme` meta. With no cookie, CSS follows `prefers-color-scheme`.
- **Wordmark:** "Klotho" as text only, Atkinson Hyperlegible Next 650 at 1.125rem (P8, the spindle mark, was rejected for now).

### Icons [P9]
One set, Lucide (through Topcoat's Iconify support, embedded at build time), `currentColor`, with Lucide's own stroke. 16px in buttons, alerts, menus and the select; 14px in badges and the checkbox. In use: `menu`, `circle-check`, `circle-alert`, `check`, `chevron-down`, `monitor`, `sun`, `moon`, plus `triangle-alert` in the Warning badge. No emoji or Unicode glyphs as icons.

## Do's and Don'ts

### Do:
- Use theme tokens through Tailwind classes (`bg-primary`, `text-muted-foreground`, `border-input`).
- Keep every page working with JS off: real links, real `POST` forms, `<details>` for disclosure, confirmations as a page state.
- Put refs, hashes, paths, scopes and secrets in mono. Put everything else in the sans.
- Check every new colour pair for AA in both themes before it ships.
- Write copy that names the action and the consequence ("Revoke", "It won't be shown again").
- Build forms from the shared pieces: `field` in a 16px stack, `form_actions` 24px below, `form_error` (or a `notice`) above.

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
| P1 | Natural-dye palette: indigo (action), madder (danger), verdigris (success), weld (the thread), cool wool-grey neutrals | Accepted, then replaced the same day by Nyx: night and spun gold (action and the thread), bronze by lamplight, madder (danger), olive (success), warm neutrals by day |
| P2 | The thread as the one signature move: the header's 2px edge, the current-nav marker, and later the commit log spine and the thread view | Accepted for the header, the current-nav marker and the commit log only. Thread and beads are `aria-hidden` |
| P3 | Warning colour | Accepted, moved to amber (hue 55–60) so it cannot be mistaken for the thread. Always an icon plus text, never a line |
| P4 | Atkinson Hyperlegible Next and Mono, self-hosted | Accepted. `--font-weight-semibold: 650` in `@theme` |
| P5 | Mobile header folds extra links into a `<details>` menu | Accepted |
| P6 | Frame widens to `max-w-6xl`; forms stay `max-w-md` | Accepted. Long code lines scroll inside their own block, never the page (rubric R16) |
| P7 | Theme toggle as a cookie set by a `POST /-/theme` form (UI-only preference, no API endpoint, like account security) | Accepted (NFR-UI-005 asks for the override). The UI-only exception is recorded in ADR 0005 |
| P8 | Authored spindle mark next to the wordmark | Rejected at first; added with Nyx (it was part of the chosen design): an inline `aria-hidden` SVG whose thread is `--thread` |
| P9 | Lucide as the one Iconify icon set | Accepted |
| P10 | Nyx: night header in both themes, gold at night and bronze by lamplight, Cormorant Garamond for page titles and the wordmark | Accepted (chosen from the "Klotho redesign" design canvas) |
