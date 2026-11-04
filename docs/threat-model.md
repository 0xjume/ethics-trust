# Threat model (honest)

- **MPC network.** Chain Signatures nodes could collectively sign anything. Base assumption of NEAR.
- **Watcher / relayer.** NEAR cannot read Solana. A lying watcher can delay the raise and can still skew credits among *registered* contributors by mis-reporting deposits. It cannot set `tokens_bought`. That basis is the contract's first-buy quote, stored at `request_launch` and checked again on confirm. Sum of floor shares must be ≤ that basis, so an early claim cannot take a later claimer's floor. It cannot send funds to an address the contract did not fix, and it cannot redirect a claim off the pubkey stored at `register`. If the stored curve does not match the Solana config, a high quote makes `minimum_amount_out` fail the swap; a low quote under-allocates and the surplus stays in the raise ATA.
- **Claim grief.** `request_distribute` is permissionless. One 0.01 NEAR deposit locks only the contributors included in that message (at most 5) for 150 s. It does not freeze the rest of the raise.
- **Rounding dust.** `floor` shares leave `tokens_bought − Σ floors` in the raise ATA. That dust is not claimable and is not paid to a last recipient.
- **Sybil.** The 1% cap is per wallet. One person can split wallets. Demo-grade, not a fairness guarantee.
- **DBC config.** Created once; the Trust pins its pubkey. `feeClaimer` is the fee vault. `leftoverReceiver` is the raise vault. Bought tokens sit in the raise Tokenkeg ATA (mint `trust/<id>/mint`, 6 decimals).
- **Replay.** Each shape is built so a second landing fails or is harmless. The ledger counts signed amounts as spent.
- **Priority fees.** Compute-budget price × limit is capped so a relayer cannot drain a vault.

Full narrative: README → Trust assumptions.
