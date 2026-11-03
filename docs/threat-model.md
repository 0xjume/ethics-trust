# Threat model (honest)

- **MPC network.** Chain Signatures nodes could collectively sign anything. Base assumption of NEAR.
- **Watcher / relayer.** NEAR cannot read Solana. A lying watcher can delay the raise and skew pro-rata among *registered* contributors. It cannot send funds to an address the contract did not fix.
- **Sybil.** The 1% cap is per wallet. One person can split wallets. Demo-grade, not a fairness guarantee.
- **DBC config.** Created once; the Trust pins its pubkey. `feeClaimer` and `leftoverReceiver` are the fee vault.
- **Replay.** Each shape is built so a second landing fails or is harmless. The ledger counts signed amounts as spent.
- **Priority fees.** Compute-budget price × limit is capped so a relayer cannot drain a vault.

Full narrative: README → Trust assumptions.
