# Design — Ethics Launch tokens for Keyless Trust

Source: public ethics.ltd `site.css` and Trust page, plus `brief.md` from the
Ethics Launch design pack. Components are **re-implemented**. Markup from the
live site is not copied.

## Palette

| Token | Hex | Role |
| --- | --- | --- |
| `ethics-night` | `#0B0D0F` | Canvas |
| `ethics-graphite` | `#12161A` | Panels |
| `ethics-pine` | `#16211D` | Deep green tint |
| `ethics-slate` | `#2A3138` | Borders / lines |
| `ethics-fog` | `#A7B0B7` | Secondary text |
| `ethics-silver` | `#D9DEE3` | Primary text |
| `ethics-teal` | `#38BFA7` | Buttons, labels, sparkle |
| `ethics-jade` | `#63D6B4` | Hover / highlight |
| `ethics-violet` | `#7F7BFF` | Rare glow |

## Type

- Display: **Cormorant Garamond** (500), tight tracking, left-aligned headlines.
- Body / UI: **DM Sans**.
- Labels: uppercase, ~0.65rem, letter-spacing 0.18–0.22em, teal.

## Shape

- Cards: `ethics-panel` — 0.75–1rem radius, 1px slate border, graphite at 80%.
- Buttons: 0.5rem radius. Primary = teal fill + night text; hover jade. Ghost = slate border.
- Pills / chips: `rounded-full`.
- Page: faint 48px teal grid, pine radial at the top, soft teal glow.

## Trust page structure (adapted)

Copy style from Ethics Trust, rewritten for keyless vaults:

1. Eyebrow `Keyless Trust`
2. Headline: **Raise a launch together. No one holds the vaults.**
3. Three steps:
   - **01 Back the raise** — own derived deposit, 1% cap, refund if the deadline misses
   - **02 The contract launches it** — Meteora DBC create-and-buy signed by NEAR
   - **03 Holders get the tokens** — pro-rata; stretch 70/20/10 fee split

Files: `packages/ui/tokens.css`, `packages/ui/tailwind.theme.cjs`, placeholder `app/`.
