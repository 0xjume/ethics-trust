# Meteora skill (pointer, not vendored)

Do not copy the Meteora Agent Skill into this repository. Load it from upstream:

- Skill root: https://github.com/MeteoraAg/meteora-invent/tree/main/skills/meteora
- Docs: https://docs.meteora.ag/agents/skill
- Required reads before DBC or DAMM work: `SKILL.md`, `references/dbc.md`, `references/damm-v2.md`, `references/wallets-and-txs.md`

The skill currently pins `@meteora-ag/dynamic-bonding-curve-sdk@1.5.11`. This repo uses the newest SDK that matches the DBC program on Solana **devnet**. See `docs/DECISIONS.md`. Where the skill's 1.5.11 examples conflict with that pin, follow the newer SDK.
