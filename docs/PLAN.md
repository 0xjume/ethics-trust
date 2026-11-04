# Keyless Trust: build plan (NEAR HACKATHON, track `memecoins`)

Status: research plan plus day-1 scaffold, milestone-2 chip-in / refund / sweep, and milestone-3 DBC Shape C launch. No funded accounts or spend in the agent env; deploy script is documented. SDK pins and spike numbers: `docs/DECISIONS.md`, `docs/spikes.md`.
Prepared 3 Nov 2026 (WIB). Scope chosen by Jume: Track A only, a separate hackathon entry, testnet and devnet only, submitted before 15 Nov 2026 06:59 WIB.

> One line: community token raises on Solana where no person or server holds the vaults. A NEAR contract controls every vault through Chain Signatures and will only ever sign five transaction shapes: refund, capped sweep, Meteora DBC create-and-buy, pro-rata distribution, and (stretch) the 70/20/10 fee split.

---

## 0. Hackathon rules that shape this plan (verified 3 Nov 2026)

Sources: [HACKATHON.md](https://nearhackathon.com/HACKATHON.md), [nearhackathon.com FAQ](https://nearhackathon.com/), [NEAR blog announcement, 19 Oct 2026](https://www.near.org/blog/near-hackathon-build-beyond-boundaries), [track prompt 11-memecoins.md](https://github.com/nearai/near-hackathon-prompt-examples/blob/main/tracks/11-memecoins.md).

| Rule | Exact wording | Source |
| --- | --- | --- |
| Deadline | "Development ends (submission deadline) 2026-11-14T23:59:00Z" = **15 Nov 06:59 WIB**. Repo public by the same time. | HACKATHON.md, Facts |
| Submission count | "All times are UTC. One submission per project." / "A submission cannot be edited" / "Each repository can be submitted only once." | HACKATHON.md |
| Team wording | "Each team gets one submission, and it cannot be edited after you send it." | NEAR blog |
| One person, two projects | "Q-015. One person, several projects? Allowed if projects are independent and disclosed: each README names the other project. Not the same project twice. One person, at most 2 different projects." | HACKATHON.md |
| Email | "Contact email (max 160 characters). One person can send up to 2 submissions, for different projects that are independent and disclosed (see Q-015)." API: "at most 2 submissions per email"; 409 if "this email already has 2 submissions". | HACKATHON.md |
| Payout | "NEAR account for the prize payout, mainnet: a named account such as `team.near` or a 64-character implicit account. The team creates it … Never create or invent one on their behalf." | HACKATHON.md |
| Demo post | The human publishes an X post that "says the project was built at NEAR HACKATHON, tags @NEARProtocol and #NEARHackathon … and has the demo video or screenshots attached." | HACKATHON.md |
| Code from before 1 Nov | "Q-001 … Yes, from the announcement on 2026-10-19 … Earlier commits are baseline." | HACKATHON.md |
| Existing product | "Q-002 … Allowed. Only the NEAR integration built in the window is judged. Disclose in README." | HACKATHON.md |
| New repo | "Q-003 … No. Dedicated branch, or tags pre-hackathon and hackathon-submission, plus a README section linking the diff." | HACKATHON.md |
| Libraries / AI code | "Q-005 … All allowed, including own pre-hackathon libs and AI code. List in README; own old libs count as baseline." | HACKATHON.md |
| Testnet | "Q-010. Testnet-only projects eligible? Yes, all tracks." | HACKATHON.md |
| Local demo | "Q-012 … public repo, reproducible README, X post with a video; screenshots are not enough when judges cannot open the demo. Hosted demo is a plus; keep it up to 2026-11-18, ideally 2026-11-25." | HACKATHON.md |
| License | "Q-014 … Any. MIT is fine. Repo stays public through judging; add a LICENSE file." | HACKATHON.md |
| Judging | "Claude Code & Codex screen every project first: they check that it runs and does what the README says. The human judges above make the decisions. Previous NEAR hackathons evaluated technical implementation, design, impact, originality and sustainability." Q-017: "Screening, not deciding." | FAQ / HACKATHON.md |
| README | "what the product is, the problem it solves, and how to try it as 'do X - see Y' steps a judge can follow. No marketing." Blog: name the NEAR tech used. | HACKATHON.md / blog |
| Track | `memecoins`: "treasuries with rules … Judged on the mechanic, not the chart." Prompt asks for "the mechanic in the contract, not in a backend somebody could switch off", "All parameters … set at launch and readable by anyone. No admin key", and "Sybil resistance good enough for a demo, with its limits written plainly in the README." | site / prompt |

**What this means for the "separate team" question.** The rules count per *project* and per *email*, not per person. Jume (djume.near, already entering Sana Guard) may legally lead both entries with the same email and the same payout account, as long as the two projects are independent and each README names the other. The blog's "each team gets one submission" is the reason to make Keyless Trust a visibly separate team anyway. Separation checklist (all must be true):

- [ ] **Project**: different name, idea and track (`memecoins`, optional `open`/`social`) from Sana Guard.
- [ ] **Repo**: its own public repo, submitted once, never the Sana Guard repo or a branch of it. MIT LICENSE file.
- [ ] **Code**: no source shared with Sana Guard (no copied modules, no shared private crate). Public libraries (near-sdk, @solana/kit, DBC SDK, chainsig.js) are fine and listed in the README. Nothing from private `0xjume/ethics`; Ethics is described as the "existing product" (Q-002) and only patterns are reused.
- [ ] **Lead name**: a named lead for this entry (Jume or a teammate). Jume decides.
- [ ] **Email**: either a different email, or Jume's email as its 2nd and final submission (then no third project is possible from that email).
- [ ] **Payout**: a mainnet NEAR account created by the team itself (e.g. a new `keylesstrust.near` or reuse `djume.near`). We do not create it.
- [ ] **NEAR contract accounts**: a separate testnet account for the Trust contract (e.g. `keyless-trust.<x>.testnet`). This also matters technically: Chain Signatures derives addresses from the *calling account*, so the two projects never share derived keys.
- [ ] **Demo post**: its own X post with video, tagging @NEARProtocol and #NEARHackathon; Jume posts it.
- [ ] **Disclosure**: both READMEs carry a "Related project by the same builder" line naming the other.
- [ ] **Submission**: its own `requestId`, payload printed and confirmed by Jume before the POST.

---

## 1. Architecture

```
 Contributor (Phantom/Solflare, Solana devnet)
     │ 1. sends SOL to their own derived deposit address
     ▼
 deposit address  ◄── path "trust/<id>/in/<solana_pubkey>"
     │ refund (to owner only)  │ sweep (≤ cap to raise vault, remainder to owner)
     ▼                         ▼
 contributor            raise vault  ◄── path "trust/<id>/raise"
                               │ DBC create-and-buy (payer + buyer)
                               ▼
                       Meteora DBC pool (config pinned at create_trust) ──► DAMM v2 on graduation
                               │ creator trading fees
                               ▼
                         fee vault  ◄── path "trust/<id>/fees"  (feeClaimer + 1.5% fill fee)
                               │ (stretch) 70/20/10 split only

 NEAR testnet:  keyless-trust contract ──sign(payload_v2:{Eddsa}, path, domain_id:1)──► v1.signer-prod.testnet
 Off-chain:     relayer/watcher (permissionless, anyone can run it): pays NEAR gas, reads Solana,
                builds the unsigned message, asks the contract to sign, submits the signed tx to Solana.
 UI:            Trust room (Next.js) showing addresses, state, ledger, explorer links, "who can move funds".
```

Components:
1. **`contracts/keyless-trust`** (Rust, near-sdk 5.x): Trust registry, state machine, ledger, Solana message parser and the five shape checkers, MPC sign calls with callbacks, events (NEP-297 style logs) for the UI log.
2. **`relayer`** (TypeScript, `@solana/web3.js` v1 + near-api-js; **not** `@solana/kit` in DBC builders): watcher, message builder, signer client, Solana submitter. Holds a NEAR key for gas and nothing else on Solana.
3. **`app`** (placeholder Trust room this PR; Next.js later): Ethics Launch tokens only.
4. **`scripts`**: create the devnet DBC config, end-to-end demo script, attack script.

No admin key: the contract is deployed, initialised with `mpc_contract`, `dbc_program`, pinned constants, and then all full-access keys are removed (or the account is locked) before the demo. Every Trust's parameters are immutable after `create_trust` and readable through views.

## 2. Address derivation

- Chain Signatures derives a child key from the MPC root key, the **predecessor account** (the Trust contract) and a path string. For Ed25519 (domain 1) the derived public key *is* the Solana address (base58).
- Paths (fixed by the contract; callers cannot choose them):
  - `trust/<id>/raise` — raise vault (buyer + DBC `poolCreator` of the first buy, leftover receiver; init/ATA rent is paid by the relayer)
  - `trust/<id>/fees` — fee vault (`feeClaimer`, receives the 1.5% fill fee and later creator trading fees)
  - `trust/<id>/mint` — deterministic base mint (MPC-signed; no throwaway keypair)
  - `trust/<id>/in/<contributor_solana_pubkey_base58>` — one deposit address per contributor
- Verified 3 Nov 2026 on testnet: `v1.signer-prod.testnet.derived_public_key({"path":"trust/1/raise","predecessor":"example.testnet","domain_id":1})` returns `"ed25519:B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj"`. Mainnet signer: `v1.signer` (not used in this entry).
- **How the contract learns addresses without curve math in WASM**: `create_trust` and `register` make a cross-contract call to `derived_public_key` with `predecessor = env::current_account_id()` and store the result in the callback (`raise`, `fees`, and `mint`). No one supplies addresses, so a relayer cannot substitute one. (Alternative: compute the Ed25519 tweak in-contract with curve25519-dalek; skip unless the cross-contract call is a problem.)
- **Registration**: `register(trust_id, solana_pubkey)` is permissionless. It is safe for anyone to register any pubkey, because the deposit address is derived from that pubkey and refunds can only ever go to it. Optional anti-spam: require a Solana `signMessage` over `keyless-trust:<contract>:<trust_id>:<pubkey>` checked with `env::ed25519_verify`. The MVP leaves this out (cheaper, and keeps the design independent from Sana Guard).

## 3. Signing pipeline and on-chain checks

Flow for every action: relayer builds an unsigned Solana **legacy or v0 message without lookup tables** → calls `request_<action>(trust_id, ..., message_bytes)` on NEAR with 1 yoctoNEAR → contract parses the message, runs the shape checker for that action and state, updates its ledger optimistically → `Promise` to `v1.signer-prod.testnet.sign({"request":{"path", "payload_v2":{"Eddsa": hex(message)}, "domain_id":1}})` → callback stores/logs the 64-byte signature keyed by `(trust_id, action, sha256(message))` → relayer assembles and submits.

Constraints:
- Payload is the full serialized message (Ed25519 signs the message, not a hash): 32–1232 bytes. Solana v1 transactions (4096 bytes, live on mainnet since 15 Sep 2026) **cannot** be used through this path; stick to legacy/v0 ≤ 1232 bytes total transaction.
- Deposit 1 yoctoNEAR per `sign`. Gas: handoff says ≥10 Tgas (~15 on mainnet) attached to `sign`; the near-smart-contracts skill uses 250 Tgas. Measure on day 1. If it is near 250 Tgas, a NEAR tx (max 300 Tgas) can only carry one `sign`, so multi-signer messages need one NEAR call per signer path.

### 3.1 Generic message checks (all shapes)
Parse header (`num_required_signatures`, readonly counts), static account keys, `recent_blockhash`, compiled instructions. Reject if:
- version is v1, or v0 with any address-table lookup (all accounts must be static so the contract can see them);
- any account required to sign is a Trust-derived key other than the one(s) this shape allows;
- any instruction targets a program not in the shape's allow-list (System, SPL Token, Associated Token, ComputeBudget, DBC `dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN`, Metaplex metadata only via DBC CPI);
- ComputeBudget price × limit > a fixed max fee (e.g. 0.0002 SOL) — stops a relayer draining a vault through priority fees;
- any instruction other than those the shape lists, in the listed order.

**Fee payer** = account key 0. Two allowed modes:
- the vault being spent (it pays its own 5,000-lamport fee; contract caps priority fee as above), or
- a relayer key that co-signs (lets the vault balance stay exact). The relayer key must not be writable in any instruction except as fee payer. Milestone 2 default: relayer pays; vault-paid still allowed.

**Blockhash**: NEAR cannot check freshness, and does not need to: a stale blockhash only makes the tx fail. The real danger is *double execution* when the same intent is signed twice with two blockhashes. Each shape is therefore designed so that a second landing either fails or does nothing harmful (see per-shape notes). If day-1 latency (MPC sign ~2–3 s documented, plus NEAR finality) approaches the ~60–90 s blockhash lifetime, switch to a **durable nonce** per vault: message's first instruction is `AdvanceNonceAccount(nonce_acct, authority = vault)`, contract checks it, and a signed message then never expires, so the relayer can retry forever without re-signing.

### 3.2 Shape A — Refund (contributor deposit → contributor)
- Signer: `in/<pubkey>` only. Instruction: `System::Transfer{from: deposit, to: <pubkey>, lamports: any}` (+ optional ComputeBudget).
- Destination is fixed by the path, so the amount does not matter and re-signing is always safe. Allowed in states `Open` (before sweep: contributor can withdraw anytime, optional) and `Refunding`, and for any *remainder* after a sweep in any state.

### 3.3 Shape B — Capped sweep (deposit → raise vault, remainder → owner)
- Signer: `in/<pubkey>`. Instructions exactly: `Transfer(deposit → raise_vault, x)` then optional `Transfer(deposit → <pubkey>, rest)`.
- **Cap**: `x ≤ cap_lamports − swept[pubkey]`, where `cap = target × cap_bps / 10_000`. `cap_bps` is **required** at `create_trust` (1..=10_000; 100 = 1%). `Σ swept ≤ target`. The contract does **not** debit `swept` on `sign`. It locks a pending row keyed by `sha256(message)` and only books on `confirm_transfer` after Solana lands. A second sign of the same kind is blocked while that lock is live. After ~150s (past the Solana blockhash lifetime) anyone may `release_pending` so a never-sent / expired tx can be re-signed. A later sign of the same lock increments a per-lock nonce and marks earlier released rows superseded (`pending superseded`). An honest late confirm with no re-sign still books.
- Overpayment never enters the raise: the remainder goes back in the same tx.
- Allowed only in state `Executing`.

### 3.4 Shape C — DBC create-and-buy (raise vault + mint)
- Built by `DynamicBondingCurveClient.creator.createPoolWithFirstBuy` (SDK 2.0.0 ↔ program 0.2.2), then decompiled to a message. Allowed instructions: optional ComputeBudget; exactly one fill-fee System transfer (raise → fee vault, exact 1.5%) unless already paid; exactly one wrap System transfer (raise → raise WSOL ATA, amount = `credited − fill_fee`) followed by `SyncNative` on that ATA; ATA `CreateIdempotent` (payer = **relayer fee payer only**, owner = raise, mint = WSOL or trust mint, ATA = derived); `initialize_virtual_pool_with_spl_token` (16 accounts, all pinned; init payer = relayer); `swap`/`swap2` (15 IDL accounts, all pinned; optional 16th = `Sysvar1nstructions` when `enableFirstSwapWithMinFee`). Index 12 is the Anchor optional `referral_token_account`: it must be the DBC program id (`None`). A real referral ATA is rejected. Event authority is index 13, DBC program index 14. Swap data is exact-in only (`swap` = 24 bytes `{amount_in, minimum_amount_out}`; `swap2` = 25 bytes with `swap_mode = 0`). `minimum_amount_out ≥ 1` (the relayer builder pins 1; 0 is rejected). Trailing bytes rejected. Optional Token `closeAccount` only on the raise WSOL ATA with destination and authority = raise. The relayer builder retargets SDK ATA payers from raise → relayer so rent is not taken from the vault.
- `request_launch`, `request_fill_fee`, and `request_sweep` are **watcher-only** so an unprivileged account cannot lock a pending for 150 s. `request_refund` / `request_raise_refund` stay permissionless (liveness if the watcher is offline) but must pay the **full** refundable amount (or the rent-dust close-out).
- The contract **derives** pool (`"pool"`, config, ordered WSOL/mint), token vaults (`"token_vault"`), Metaplex metadata, event authority, and both raise ATAs, then compares every account by index. Relayer (fee payer) pays init + ATA rent so the raise vault does not need a manual SOL top-up. Unknown programs, short account lists, extra raise transfers, or a redirected swap output ATA are rejected.
- `base_mint` must be a signer. It is the MPC-derived key at `trust/<id>/mint` (no random keypair). The contract signs the same message twice (`Promise::and` of raise + mint, ~90 TGas attached). Relayer is the Solana fee payer. Result: 2 MPC signatures + 1 relayer signature on one message.
- Fill fee: exact 1.5% raise → fee vault, either in this tx or a prior `request_fill_fee` shape.
- Double execution: a second landing fails because the pool PDA already exists.
- Allowed only in `Executing` after all sweeps are reported landed.
- Launch confirm is `confirm_launch_outcome(message_hash, solana_sig, tokens_bought)`. `confirm_transfer` stays two arguments so sweep and refund JSON does not gain a field.

### 3.5 Shape D — Pro-rata distribution (raise vault → contributors)
- Allowed only in `Launched`, after `tokens_bought` is recorded. Signer: raise vault (`trust/<id>/raise`) only. Relayer is fee payer (account 0) and the only other signer. The relayer is also writable as the destination-ATA rent payer, same as Shape C. Legacy message, no address tables, no ComputeBudget, ≤ 1232 bytes, one raise MPC sign.
- Token program is classic Tokenkeg. Shape C initializes with `initialize_virtual_pool_with_spl_token` and rejects Token-2022, so distribution uses Tokenkeg only: ATA seeds `[owner, Tokenkeg, mint]`, TransferChecked decimals 6. Mint is the derived `trust/<id>/mint` key. The program id is a constant, not a stored field.
- Each leg is an optional `CreateIdempotent` (data exactly `[1]`, 6 accounts: relayer, dest ATA, owner, mint, System, Tokenkeg) then one `TransferChecked` (data exactly 10 bytes `[12, amount u64 LE, decimals = 6]`, accounts: raise token ATA, mint, dest ATA, raise authority). No other instructions. Destination owner is the contributor pubkey stored at `register` (the same key refunds pay). A contributor cannot redirect to another owner. Up to 5 legs per call.
- Share = `floor(tokens_bought × credited / credited_total)`. The basis is **credited** lamports, not swept. Dust (`tokens_bought − Σ floors`) stays in the raise ATA and is not a share. Each remainder is < 1 raw unit, so dust is strictly less than the number of credited contributors. It is not paid to a last recipient.
- `tokens_bought` is the raise-ATA balance the watcher reports on `confirm_launch_outcome`, bounded to `1..=1_000_000_000 × 10^6` (the pinned 1B supply at 6 decimals). The contract does not store curve parameters, so it cannot recompute the buy. Under-report scales every share down and leaves the residual in the raise ATA. Over-report up to the cap can make a later transfer fail on Solana if the ATA is short. The watcher cannot change who is paid.
- Nothing is booked at sign. The pending row is keyed by `sha256(message)`, with a per-contributor lock and nonce, 150 s expiry, `release_pending`, `seen_sigs`, and `consumed_hashes`. A re-sign supersedes the released row, so release plus a new sign cannot double-pay. `distributed_tokens` (per contributor) and `distributed_total` update only on `confirm_transfer`. The trust moves to `Distributed` when `distributed_total >= claimable_total` (the sum of floors). If that sum is 0 at launch confirm, the trust goes straight to `Distributed`.
- Permissionless, with a per-contributor lock. A stranger who pays the 0.01 NEAR `sign_deposit` (excess refunded) can freeze only the contributors named in that message — at most 5 — for 150 s. Leaving someone out does not freeze them. Attach at least 60 TGas (`distribute_attach_tgas`: one 30 TGas `sign`, a 20 TGas callback, and checker headroom).
- Views: `claimable(trust_id, contributor)`, `distribution_status` (`tokens_bought`, `claimable_total`, `distributed_total`, `dust_tokens`, decimals, token program).

### 3.6 Shape E (stretch) — Fee split (fee vault → 3 fixed destinations)
- `claim_creator_trading_fee` (fee vault as creator) then transfers to `holders_distributor` (70%), `ethics_holders_distributor` (20%), `platform` (10%), destinations fixed at `create_trust`. Checks: ratios within 1 lamport rounding; no other destination. The "70% to holders" leg pays a distributor address; true per-holder airdrops are out of scope.

## 4. Trust assumptions (to state plainly in the README)

- **MPC network**: Chain Signatures nodes collectively could sign anything; this is the base assumption of the whole NEAR stack.
- **Watcher/relayer**: NEAR cannot read Solana. A watcher **must** be set at `create_trust`. It reports deposit balances, confirms landed sweeps/refunds/distributes (`confirm_transfer`), and the post-buy token amount (`confirm_launch_outcome`, capped at the 1B×10^6 supply). A lying or offline watcher can **delay** everything and can **skew the pro-rata split among real registered contributors** (under-report scales every floor share down; over-report up to the cap can make transfers fail if the raise ATA is short). It cannot send funds to any address the contract did not derive or to any owner other than the pubkey stored at `register`. Anyone can run a second watcher to audit. Refunds after the deadline are permissionless except that `confirm_transfer` still needs the named watcher. Distribution claims are permissionless too, but a paid claim locks only the included contributors for 150 s.
- Mitigations: multiple watchers with "first to report, others can dispute within N minutes" (stretch); every report is logged on NEAR so anyone can audit against Solana explorers; optional future: Solana light-client or ZK proofs of account state.
- **Sybil**: the 1% cap is per *wallet*; one person can split into many wallets. Good enough for a demo, not a fairness guarantee. Say so.
- **DBC config**: created once by a normal keypair with `createConfig2`; once created it cannot be changed, and the Trust pins its pubkey. `feeClaimer` = fee vault; `leftoverReceiver` = **raise vault**. No human receives partner fees.

## 5. Raise state machine

```
            create_trust
                 │
                 ▼
   ┌──────────► Raising ──(deadline passed, Σ credited < target)──► Refunding
   │             │
   │  (Σ credited ≥ target, before deadline)
   │             ▼
   │          Filled ──(24 h execution timeout)────────────────────► Refunding
   │             │ first sweep
   │             ▼
   │         Executing ──(24 h from start, not Launched)───────────► Refunding
   │             │  sweeps (B) → create-and-buy (C)
   │             ▼
   │          Launched ── pro-rata (D) ──► Distributed
   │                                          (stretch) fee split (E) after D
```
- Defaults (Ethics patterns, re-implemented): cap 1% of target per wallet, deadline set at creation, 24 h execution timeout, 1.5% fill fee to the fee vault (taken from the raise before the buy; MVP may set 0), fee split 70/20/10.
- `Refunding`: any caller can request Shape A for any registered deposit; also covers deposits that arrive late.
- Transitions are triggered by anyone; time checks use `env::block_timestamp()`.

## 6. Meteora DBC details

- Program `dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN` (same id on mainnet and devnet, per Meteora docs). DAMM v2 `cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG`. SDK `@meteora-ag/dynamic-bonding-curve-sdk@2.0.0` (client) and `/math` (curve, fee, swap quote) — confirmed the 2.0.0 tarball embeds the IDL with the discriminators above.
- Demo config (devnet, SOL quote; the NEAR-quoted Ethics curve with 609 NEAR initial / 5,483 NEAR migration cap only exists on mainnet, so scale it): fixed 1B supply, classic SPL (`tokenType` SPL), `transferFeeParameters: null`, migration to DAMM v2, LP locked, curve built with the SDK builder so that a ~2 SOL raise buys a meaningful slice and graduation happens at e.g. 10 SOL (or set the raise ≥ migration threshold to demo graduation). `enableFirstSwapWithMinFee` on, so the Trust's first buy pays the minimum fee (check semantic on day 1). `feeClaimer` = fee vault; `leftoverReceiver` = raise vault.
- Only `createPoolWithFirstBuy` is used (never `createConfigAndPoolWithFirstBuy`, which returns two txs and would need the config key in the Trust's signing set).

## 7. Open technical risks

| # | Risk | Why | Mitigation / spike |
| --- | --- | --- | --- |
| 1 | DBC create-and-buy > 1232 bytes | ~19 unique accounts × 32 B ≈ 610 B, 3 signatures 192 B, name/symbol/uri, ~6 instructions. Estimate ~1,000–1,150 B, tight. Lookup tables are banned by our parser. | Day-1 measure with real SDK output. Short URI (≤ 60 chars). Drop ComputeBudget ixs. Fallback: split into tx1 `initialize_virtual_pool` (fee vault + throwaway mint) and tx2 `swap` (raise vault); risk a sniper buys between them → mitigate with DBC activation delay / fee scheduler or accept for demo. |
| 2 | Parsing cost in WASM | Manual parser of compact-u16 arrays and fixed layouts is O(size), no allocation-heavy deps; expect a few Tgas. Borsh on IDL args only. | Measure in `near-workspaces` sandbox; keep parser hand-written (no `solana-sdk` crate; it bloats WASM). |
| 3 | DBC instruction layouts | Account order and args can change between program versions (0.2.2 now). | Generate fixtures from the SDK 2.0.0 IDL; pin program version in the README; contract stores account *positions* per instruction as constants with tests built from real SDK output. |
| 4 | MPC latency vs blockhash lifetime | sign is async (yield/resume), ~2–3 s documented, up to minutes under load; 3 MPC sigs for Shape C | Fetch blockhash as late as possible; parallel sign calls; durable nonces if p95 > 30 s. |
| 5 | Gas per sign / multiple sign calls per NEAR tx | 300 Tgas cap per tx | Measure; one `request_sign(path)` per call if needed; signatures cached by message hash. |
| 6 | Double-sign / replay | Same intent, new blockhash | Shapes designed to fail or be harmless on second landing (§3); ledger counts signed as spent. |
| 7 | Watcher honesty | NEAR blind to Solana | §4 disclosure; bounded destinations. |
| 8 | Token-2022 | transfer fees/hooks change amounts and accounts | Base token classic SPL only; quote = wSOL; reject Token-2022 program ids in all shapes. |
| 9 | Devnet availability | DBC on devnet with custom config, DAMM v2 graduation on devnet | Day-1 spike: create config + pool + first buy on devnet with a plain keypair. Fallback: demo C on a local Surfpool/LiteSVM fork with the mainnet program cloned. |
| 10 | Testnet MPC capacity | testnet signer outages | Record the demo early; keep a recorded run. |
| 11 | Workload | Two Chain Signatures entries by the same builder in 11 days | Strict MVP; E is stretch; UI minimal. |

## 8. Milestones (3–14 Nov 2026 WIB; submit by 15 Nov 06:59 WIB)

| Day | Date (WIB) | Work | Done when |
| --- | --- | --- | --- |
| 1 | Tue 3 Nov | **Spikes**: (a) NEAR testnet throwaway contract calls `sign` domain 1 for a devnet SOL transfer from `trust/0/raise`, measure gas + latency; (b) DBC on devnet: `createConfig2` + `createPoolWithFirstBuy` with a plain keypair, measure tx size; (c) decide blockhash vs durable nonce | A devnet transfer signed by a NEAR contract lands; DBC pool exists on devnet; size and latency numbers written in `docs/spikes.md` |
| 2 | Wed 4 Nov | Repo scaffold (after approval), contract skeleton: `create_trust`, `register`, `derived_public_key` callback, state machine, views, events | Unit tests pass; addresses match chainsig.js derivation |
| 3 | Thu 5 Nov | Solana message parser + generic checks + Shapes A and B with fixtures | Disallowed shapes rejected in tests (attacker destination, extra ix, LUT, fee too high) |
| 4 | Fri 6 Nov | Relayer: watcher (balances), builder, sign client, submitter; refund end to end | Devnet refund lands from a contract-signed message |
| 5 | Sat 7 Nov | Shape C checker using SDK-generated fixtures | Real SDK message accepted; 10 mutated variants rejected |
| 6 | Sun 8 Nov | Execute end to end: sweeps → create-and-buy on devnet | Devnet DBC pool created by Trust vaults |
| 7 | Mon 9 Nov | Shape D distribution + reconciliation; timeouts → Refunding | Tokens reach 2+ contributors pro-rata; timeout test passes |
| 8 | Tue 10 Nov | Trust room UI: addresses, progress, ledger/event log, explorer links, "who can move funds" panel | Full demo clickable locally |
| 9 | Wed 11 Nov | One-command e2e (`pnpm demo`) + attack script; README "do X, see Y"; lock contract keys | A fresh clone reproduces the demo on testnet/devnet |
| 10 | Thu 12 Nov | Stretch: Shape E fee split, or multi-watcher dispute. Feature freeze at end of day | Either merged with tests or dropped |
| 11 | Fri 13 Nov | Record demo video; draft X post; README names Sana Guard; LICENSE | Video file ready |
| 12 | Sat 14 Nov | Buffer. Repo public, Jume posts on X, Jume confirms payload, submit | 201 from submission API before 15 Nov 06:59 WIB |

Minimal demo (≈3 min): create Trust (NEAR testnet) → two wallets chip in on devnet, one overpays → execute: capped sweeps, remainder auto-returned → DBC pool appears on devnet, tokens distributed → second Trust misses deadline, a third party triggers refunds → "attack": ask the contract to sign a transfer to an attacker address, show the NEAR rejection.

## 9. Proposed repo layout

```
keyless-trust/
  README.md            # what / problem / run / do X see Y / NEAR tech / trust assumptions / related project: Sana Guard
  LICENSE              # MIT
  contracts/keyless-trust/
    src/lib.rs         # entrypoints, state machine
    src/solana/        # message parser, compact-u16, instruction decoders
    src/shapes/        # refund.rs, sweep.rs, dbc_create_buy.rs, distribute.rs, fee_split.rs
    src/mpc.rs         # sign + derived_public_key promises and callbacks
    tests/fixtures/    # real messages produced by the SDK
  relayer/             # TS: watcher, builders (DBC SDK 2.0.0, @solana/kit), submitter
  app/                 # Next.js Trust room
  scripts/             # create-devnet-config.ts, demo.ts, attack.ts
  docs/                # spikes.md, threat-model.md, shapes.md
```

## 10. Regulatory risk (honest assessment, not legal advice)

- A pooled pre-launch raise where many people send money to buy a new token, with the expectation it trades higher, looks like an investment contract / securities offering under the US Howey test and under similar tests elsewhere; it can also look like unlicensed crowdfunding or collective investment. Making it "keyless" improves custody safety but does not change the legal nature; arguably it removes the custodian who would otherwise carry obligations, which regulators do not treat as an exemption.
- Indonesia (Jume's base): crypto assets are now supervised by OJK (moved from Bappebti in Jan 2025); offering tokens to the public and operating a crypto platform without licences carries risk. EU: MiCA covers public offers of crypto-assets (memecoins are mostly out of the white-paper regime only in narrow cases). US: SEC staff stated in 2025 that typical memecoins are not securities, but pooled raises with promoter efforts are a different fact pattern.
- For the hackathon: devnet/testnet only (no real value), no promises of returns, language framed as "escrow rules / refunds guaranteed by code", a README disclaimer, no Ethics/$ETHICS token promotion in the demo. Before any mainnet use: legal advice in the operating jurisdictions, geofencing, and a consumer-protection review.

## 11. Decisions and approvals Jume needs to give

1. **Repo**: name (suggest `keyless-trust`) and owner (personal `0xjume`, a new org, or a teammate's account). Creating it and making it public need Jume's approval.
2. **Team identity for this entry**: lead name; contact email (a separate one, or Jume's as the 2nd and last submission); team size.
3. **Payout account**: a mainnet NEAR account Jume or the team creates (reuse `djume.near` or a new one).
4. **Network**: confirm testnet + devnet only (no mainnet funds). Demo SOL from devnet faucet; NEAR from testnet faucet.
5. **Testnet accounts**: OK to create a NEAR testnet account for the contract and a relayer gas account (free, but still an account creation).
6. **Branding**: carry the Ethics name and say today's Trust vaults are server-held, or stay neutral.
7. **Scope**: confirm Shape E (fee split) is stretch and registration has no signature check.
8. **Demo post and submission**: Jume posts the X video himself and confirms the exact submission JSON before it is sent.
