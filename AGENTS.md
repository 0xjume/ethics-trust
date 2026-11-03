# Agent instructions — Keyless Trust

This repository is a **NEAR HACKATHON 2026** entry (`memecoins`). Testnet and Solana
devnet only. Do not create funded accounts, request faucets, deploy, or spend. Do not
commit secrets. Do not copy code from private `0xjume/ethics` or any Sana Guard repo.

## Load these skills first

1. **NEAR smart contracts** — `.agents/skills/near-smart-contracts/SKILL.md`
   then the rule files it names (`structure-near-bindgen`, `chain-signatures`,
   `xcc-promise-chaining`, `security-storage-checks`, `state-collections`).
2. **NEAR contract audit** — `.agents/skills/near-contract-audit/SKILL.md` as a
   checklist before merging contract changes.
3. **NEAR TypeScript** — `.agents/skills/near-api-js/SKILL.md` (and
   `.agents/skills/near-kit/SKILL.md` when a dApp wallet is added). Use these for
   relayer and scripts, not `@solana/kit` for Meteora txs.
4. **Meteora (not vendored)** — load from
   https://github.com/MeteoraAg/meteora-invent/tree/main/skills/meteora
   (`SKILL.md`, `references/dbc.md`, `references/damm-v2.md`,
   `references/wallets-and-txs.md`). See `.agents/skills/meteora/README.md`.
5. **Design** — `docs/DESIGN.md` and `packages/ui/tokens.css` for any UI.

## Version fences

- DBC and DAMM SDK pins live in `docs/DECISIONS.md`. Follow the pinned versions,
  not the Meteora skill's older 1.5.11 examples, when they conflict.
- Meteora BUILD path uses `@solana/web3.js` v1 (`Transaction` / `PublicKey` /
  `Keypair`). Do not mix `@solana/kit` types into DBC builders.
- NEAR contracts: `near-sdk` 5.x, `#[near(contract_state)]`, `PanicOnDefault`,
  `#[init]`, unique collection prefixes, `#[private]` callbacks, 1 yoctoNEAR on
  `sign`, `request` wrapper for MPC args.

## Layout

```
contracts/keyless-trust/   NEAR contract (parser + shapes + MPC)
relayer/                   TypeScript builders, watcher stubs, spikes
app/                       placeholder Trust room (Ethics Launch tokens)
packages/ui/               design tokens + Tailwind theme
scripts/                   thin wrappers; implementations live in relayer/
docs/                      PLAN, DECISIONS, DESIGN, spikes
.agents/skills/            NEAR skills in-repo; Meteora is a pointer
```
