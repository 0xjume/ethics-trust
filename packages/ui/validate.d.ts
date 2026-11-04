export const DEMO_CURVE: {
  sqrt_start_price: string;
  migration_quote_threshold: string;
  collect_fee_mode: number;
  enable_first_swap_with_min_fee: boolean;
  base_fee_mode: number;
  cliff_fee_numerator: string;
  number_of_period: number;
  reduction_factor: string;
  points: Array<{ sqrt_price: string; liquidity: string }>;
};

export const DEFAULT_FILL_FEE_BPS: number;
export const CREATE_DEPOSIT_NEAR: string;
export const CREATE_GAS_TGAS: number;
export const REGISTER_DEPOSIT_NEAR: string;
export const SIGN_DEPOSIT_NEAR: string;
export const DISTRIBUTE_GAS_TGAS: number;

export function isSolanaPubkey(value: unknown): boolean;
export function isNearAccount(value: unknown): boolean;
export function validateCurve(curve: unknown): string[];

export function validateCreateTrust(input: Record<string, unknown>):
  | { ok: true; args: Record<string, unknown> }
  | { ok: false; errors: string[] };

export function stringifyNearArgs(value: unknown): string;

export function buildCreateTrustCli(input: {
  contractId: string;
  args: Record<string, unknown>;
  accountId?: string;
}): string;

export function buildRegisterCli(input: {
  contractId: string;
  trustId: number;
  solanaPubkey: string;
  accountId?: string;
}): string;

export function buildDistributeCli(input: {
  contractId: string;
  trustId: number;
  messageBase64: string;
  accountId?: string;
}): string;
