/** create_trust checks mirrored from the contract, plus the CLI the form prints. */

import { asBigInt } from "./format.mjs";

const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

export const DEMO_CURVE = {
  sqrt_start_price: "32609544831759054",
  migration_quote_threshold: "10000000000",
  collect_fee_mode: 0,
  enable_first_swap_with_min_fee: true,
  base_fee_mode: 0,
  cliff_fee_numerator: "10000000",
  number_of_period: 0,
  reduction_factor: "0",
  points: [
    {
      sqrt_price: "130438178253327822",
      liquidity: "34783514296328165318017769454091",
    },
  ],
};

export const DEFAULT_FILL_FEE_BPS = 150;
export const CREATE_DEPOSIT_NEAR = "0.01";
export const CREATE_GAS_TGAS = 100;
export const REGISTER_DEPOSIT_NEAR = "0.005";
export const SIGN_DEPOSIT_NEAR = "0.01";
export const DISTRIBUTE_GAS_TGAS = 60;

export function isSolanaPubkey(value) {
  const text = String(value ?? "").trim();
  if (text.length < 32 || text.length > 44) return false;
  if ([...text].some((ch) => !BASE58.includes(ch))) return false;
  return decodeBase58(text)?.length === 32;
}

export function isNearAccount(value) {
  const text = String(value ?? "").trim();
  if (text.length < 2 || text.length > 64) return false;
  return /^(([a-z\d]+[-_])*[a-z\d]+\.)*([a-z\d]+[-_])*[a-z\d]+$/.test(text);
}

export function validateCurve(curve) {
  const errors = [];
  if (!curve || typeof curve !== "object") {
    return ["Curve JSON is required"];
  }
  const points = Array.isArray(curve.points) ? curve.points : [];
  if (points.length < 1 || points.length > 16) {
    errors.push("Curve must have 1 to 16 points");
  }
  if (!positiveInt(curve.sqrt_start_price)) errors.push("sqrt start price must be > 0");
  if (!positiveInt(curve.migration_quote_threshold)) {
    errors.push("Migration threshold must be > 0");
  }
  const mode = Number(curve.collect_fee_mode);
  if (mode !== 0 && mode !== 1) errors.push("Collect fee mode must be 0 (quote) or 1 (output)");
  if (Number(curve.base_fee_mode) !== 0) errors.push("Only a linear fee scheduler is quoted");
  if (!nonNegativeInt(curve.cliff_fee_numerator)) errors.push("Cliff fee numerator is required");
  if (positiveInt(curve.cliff_fee_numerator) && asBigInt(curve.cliff_fee_numerator) > 990_000_000n) {
    errors.push("Fee numerator is above the maximum");
  }
  const periods = Number(curve.number_of_period);
  if (!Number.isInteger(periods) || periods < 0 || periods > 65535) {
    errors.push("Number of periods must be 0..=65535");
  }
  if (!nonNegativeInt(curve.reduction_factor)) errors.push("Reduction factor is required");
  points.forEach((point, index) => {
    if (!positiveInt(point?.sqrt_price) || !positiveInt(point?.liquidity)) {
      errors.push(`Curve point ${index + 1} needs sqrt price and liquidity > 0`);
    }
  });
  return errors;
}

/**
 * @param {Record<string, unknown>} input
 * @returns {{ ok: true, args: Record<string, unknown> } | { ok: false, errors: string[] }}
 */
export function validateCreateTrust(input) {
  const errors = [];
  let target;
  try {
    target = asBigInt(input.target_lamports);
    if (target <= 0n) errors.push("Target must be greater than 0 lamports");
  } catch {
    errors.push("Target lamports must be a whole number");
  }

  let deadline;
  try {
    deadline = asBigInt(input.deadline_ns);
    if (deadline <= 0n) errors.push("Deadline must be a NEAR timestamp in nanoseconds");
  } catch {
    errors.push("Deadline must be a NEAR timestamp in nanoseconds");
  }

  const dbc = String(input.dbc_config ?? "").trim();
  if (!isSolanaPubkey(dbc)) errors.push("DBC config must be a Solana pubkey");

  const name = String(input.name ?? "").trim();
  const symbol = String(input.symbol ?? "").trim();
  const uri = String(input.uri ?? "").trim();
  if (!name || !symbol) errors.push("Name and symbol are required");
  if (uri.length > 60) errors.push("URI must be 60 characters or fewer");

  const cap = Number(input.cap_bps);
  if (!Number.isInteger(cap) || cap < 1 || cap > 10_000) {
    errors.push("cap_bps is required and must be 1..=10000");
  }

  let fillFee;
  if (input.fill_fee_bps === "" || input.fill_fee_bps == null) {
    fillFee = undefined;
  } else {
    fillFee = Number(input.fill_fee_bps);
    if (!Number.isInteger(fillFee) || fillFee < 0 || fillFee > 10_000) {
      errors.push("fill_fee_bps must be 0..=10000");
    }
  }

  let timeout;
  if (input.execution_timeout_ns === "" || input.execution_timeout_ns == null) {
    timeout = undefined;
  } else {
    try {
      timeout = asBigInt(input.execution_timeout_ns);
      if (timeout <= 0n) errors.push("Execution timeout must be > 0");
    } catch {
      errors.push("Execution timeout must be a whole number of nanoseconds");
    }
  }

  const watcher = String(input.watcher ?? "").trim();
  if (!isNearAccount(watcher)) errors.push("Watcher must be a NEAR account id");

  let curve = input.curve;
  if (typeof curve === "string") {
    try {
      curve = JSON.parse(curve);
    } catch {
      errors.push("Curve JSON does not parse");
      curve = null;
    }
  }
  if (curve) errors.push(...validateCurve(curve));

  if (errors.length) return { ok: false, errors };

  const args = {
    target_lamports: target,
    deadline_ns: deadline,
    dbc_config: dbc,
    name,
    symbol,
    uri,
    cap_bps: cap,
    watcher,
    curve: normalizeCurve(curve),
  };
  if (fillFee != null) args.fill_fee_bps = fillFee;
  if (timeout != null) args.execution_timeout_ns = timeout;
  return { ok: true, args };
}

export function stringifyNearArgs(value) {
  const json = JSON.stringify(value, (_key, item) =>
    typeof item === "bigint" ? { __bigint: item.toString() } : item,
  );
  return json.replace(/\{"__bigint":"(-?\d+)"\}/g, "$1");
}

export function buildCreateTrustCli({ contractId, args, accountId }) {
  const json = stringifyNearArgs(args);
  const account = accountId && isNearAccount(accountId) ? accountId : "YOUR.testnet";
  return [
    "near call",
    contractId,
    "create_trust",
    `'${json}'`,
    "--deposit",
    CREATE_DEPOSIT_NEAR,
    "--gas",
    `${CREATE_GAS_TGAS} Tgas`,
    "--networkId testnet",
    "--accountId",
    account,
  ].join(" ");
}

export function buildRegisterCli({ contractId, trustId, solanaPubkey, accountId }) {
  const account = accountId && isNearAccount(accountId) ? accountId : "YOUR.testnet";
  const args = stringifyNearArgs({
    trust_id: Number(trustId),
    solana_pubkey: solanaPubkey,
  });
  return [
    "near call",
    contractId,
    "register",
    `'${args}'`,
    "--deposit",
    REGISTER_DEPOSIT_NEAR,
    "--gas 40 Tgas",
    "--networkId testnet",
    "--accountId",
    account,
  ].join(" ");
}

export function buildDistributeCli({ contractId, trustId, messageBase64, accountId }) {
  const account = accountId && isNearAccount(accountId) ? accountId : "YOUR.testnet";
  const args = stringifyNearArgs({
    trust_id: Number(trustId),
    message: messageBase64,
  });
  return [
    "near call",
    contractId,
    "request_distribute",
    `'${args}'`,
    "--deposit",
    SIGN_DEPOSIT_NEAR,
    "--gas",
    `${DISTRIBUTE_GAS_TGAS} Tgas`,
    "--networkId testnet",
    "--accountId",
    account,
  ].join(" ");
}

function normalizeCurve(curve) {
  return {
    sqrt_start_price: String(curve.sqrt_start_price),
    migration_quote_threshold: asBigInt(curve.migration_quote_threshold),
    collect_fee_mode: Number(curve.collect_fee_mode),
    enable_first_swap_with_min_fee: Boolean(curve.enable_first_swap_with_min_fee),
    base_fee_mode: Number(curve.base_fee_mode),
    cliff_fee_numerator: asBigInt(curve.cliff_fee_numerator),
    number_of_period: Number(curve.number_of_period),
    reduction_factor: asBigInt(curve.reduction_factor),
    points: curve.points.map((point) => ({
      sqrt_price: String(point.sqrt_price),
      liquidity: String(point.liquidity),
    })),
  };
}

function positiveInt(value) {
  try {
    return asBigInt(value) > 0n;
  } catch {
    return false;
  }
}

function nonNegativeInt(value) {
  try {
    return asBigInt(value) >= 0n;
  } catch {
    return false;
  }
}

function decodeBase58(text) {
  let value = 0n;
  for (const ch of text) {
    const digit = BASE58.indexOf(ch);
    if (digit < 0) return null;
    value = value * 58n + BigInt(digit);
  }
  const out = [];
  while (value > 0n) {
    out.push(Number(value & 0xffn));
    value >>= 8n;
  }
  for (const ch of text) {
    if (ch !== "1") break;
    out.push(0);
  }
  return Uint8Array.from(out.reverse());
}
