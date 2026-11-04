export function asBigInt(value: unknown): bigint;
export function parseNearJson(text: string): unknown;
export function formatUnits(raw: unknown, decimals: number, maxFrac?: number): string;
export function formatSol(lamports: unknown): string;
export function formatTokens(raw: unknown): string;
export function sharePercent(part: unknown, whole: unknown): string;
export function proRataShare(tokensBought: unknown, credited: unknown, creditedTotal: unknown): bigint;
export function formatNearTime(ns: unknown): string;
export function shortKey(value: unknown, head?: number, tail?: number): string;
export const TRUST_STATES: readonly string[];
export function statusLabel(state: string): string;
export function solanaExplorerAddress(address: string): string;
export function solanaExplorerTx(signature: string): string;
export function nearblocksAccount(accountId: string): string;
export function nearblocksTx(hash: string): string;
export function ledgerKeysFromState(
  values: Array<{ key?: string; value?: string } | null | undefined> | null | undefined,
): Array<{ trustId: string; contributor: string }>;
export function countContributors(rows: Array<{ trustId: string }>, trustId: unknown): number;
export function lamportsToSolInput(lamports: unknown): string;
export function solToLamports(sol: string): bigint;
export const LAMPORTS: bigint;
export const TOKEN_BASE: bigint;
