/**
 * Read-only: does the newest DBC SDK match the program on Solana devnet?
 *
 * Checks:
 *  1. npm-installed SDK version vs what we pin
 *  2. embedded IDL instruction + account discriminators
 *  3. on-chain program accounts (DBC + DAMM v2)
 *  4. on-chain Anchor IDL (when published)
 *  5. live PoolConfig / VirtualPool decode (account layout)
 */
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Connection, PublicKey } from "@solana/web3.js";
import { DynamicBondingCurveClient } from "@meteora-ag/dynamic-bonding-curve-sdk";
import { CP_AMM_PROGRAM_ID, CpAmm } from "@meteora-ag/cp-amm-sdk";
import { inflateSync } from "node:zlib";

export const DBC_PROGRAM = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN";
export const DAMM_V2_PROGRAM = "cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG";
const RPC = process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";

/** PLAN.md / program 0.2.2 instruction discriminators. */
export const EXPECTED_IX = {
  initialize_virtual_pool_with_spl_token: [140, 85, 215, 176, 102, 54, 104, 79],
  swap: [248, 198, 158, 145, 225, 117, 135, 200],
  create_config2: [9, 202, 24, 203, 143, 212, 252, 81],
};

/** Anchor account discriminators from the 2.0.0 IDL (sha256("account:<Name>")[0..8]). */
export const EXPECTED_ACCOUNTS = {
  PoolConfig: [26, 108, 14, 123, 116, 230, 129, 43],
  VirtualPool: [213, 224, 5, 209, 98, 69, 119, 92],
};

const POOL_CONFIG_FIELDS = [
  "quoteMint",
  "feeClaimer",
  "leftoverReceiver",
  "poolFees",
  "curve",
] as const;

const VIRTUAL_POOL_FIELDS = ["config", "creator", "baseMint", "quoteMint"] as const;

type IdlIx = { name: string; discriminator?: number[] };
type IdlAccount = { name: string; discriminator?: number[] };
type Idl = {
  address?: string;
  metadata?: { name?: string; version?: string };
  instructions?: IdlIx[];
  accounts?: IdlAccount[];
};

export function installedPackageVersion(name: string): string {
  const require = createRequire(import.meta.url);
  const pkg = join(dirname(require.resolve(name)), "..", "package.json");
  return JSON.parse(readFileSync(pkg, "utf8")).version as string;
}

export function anchorDisc(prefix: "global" | "account", name: string): number[] {
  return Array.from(
    createHash("sha256").update(`${prefix}:${name}`).digest().subarray(0, 8),
  );
}

function sameBytes(a: number[] | undefined, b: number[]): boolean {
  return Boolean(a && a.length === b.length && a.every((byte, i) => byte === b[i]));
}

/** Anchor 0.31 TS IDL uses camelCase; discriminators are still snake_case rust names. */
export function camelToSnake(name: string): string {
  return name
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1_$2")
    .toLowerCase();
}

export function pascalFromCamel(name: string): string {
  return name.length === 0 ? name : name[0].toUpperCase() + name.slice(1);
}

export function extractSdkIdl(connection?: Connection): {
  address: string | undefined;
  metadataVersion: string | undefined;
  instructions: Record<string, number[]>;
  accounts: Record<string, number[]>;
  instructionNames: string[];
  accountNames: string[];
} {
  const conn = connection ?? new Connection("http://127.0.0.1:1", "confirmed");
  const client = new DynamicBondingCurveClient(conn, "confirmed");
  const idl = client.program.idl as Idl;
  const instructions: Record<string, number[]> = {};
  for (const ix of idl.instructions ?? []) {
    const snake = camelToSnake(ix.name);
    instructions[snake] = ix.discriminator ?? anchorDisc("global", snake);
    instructions[ix.name] = instructions[snake];
  }
  const accounts: Record<string, number[]> = {};
  for (const account of idl.accounts ?? []) {
    const pascal = pascalFromCamel(account.name);
    const snake = camelToSnake(account.name);
    const disc = account.discriminator ?? anchorDisc("account", pascal);
    accounts[pascal] = disc;
    accounts[account.name] = disc;
    accounts[snake] = disc;
  }
  return {
    address: idl.address,
    metadataVersion: idl.metadata?.version,
    instructions,
    accounts,
    instructionNames: (idl.instructions ?? []).map((ix) => camelToSnake(ix.name)),
    accountNames: (idl.accounts ?? []).map((account) => pascalFromCamel(account.name)),
  };
}

function encodeBase58(bytes: number[]): string {
  const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
  let zeros = 0;
  while (zeros < bytes.length && bytes[zeros] === 0) zeros += 1;
  const digits = [0];
  for (const byte of bytes) {
    let carry = byte;
    for (let j = 0; j < digits.length; j += 1) {
      carry += digits[j] * 256;
      digits[j] = carry % 58;
      carry = Math.floor(carry / 58);
    }
    while (carry > 0) {
      digits.push(carry % 58);
      carry = Math.floor(carry / 58);
    }
  }
  return "1".repeat(zeros) + digits.reverse().map((d) => ALPHABET[d]).join("");
}

async function fetchOnChainIdl(
  connection: Connection,
  programId: PublicKey,
): Promise<{
  found: boolean;
  address: string;
  instructionNames: string[];
  accountNames: string[];
  instructionMatch: Record<string, boolean>;
  error: string | null;
}> {
  const [address] = PublicKey.findProgramAddressSync(
    [Buffer.from("anchor:idl")],
    programId,
  );
  const empty = {
    found: false,
    address: address.toBase58(),
    instructionNames: [] as string[],
    accountNames: [] as string[],
    instructionMatch: {} as Record<string, boolean>,
    error: null as string | null,
  };
  try {
    const info = await connection.getAccountInfo(address);
    if (!info) {
      return { ...empty, error: "no on-chain Anchor IDL account" };
    }
    const raw = info.data;
    if (raw.length < 44) {
      return { ...empty, found: true, error: `IDL account too short (${raw.length})` };
    }
    const dataLen = raw.readUInt32LE(40);
    const compressed = raw.subarray(44, 44 + dataLen);
    const json = inflateSync(compressed).toString("utf8");
    const idl = JSON.parse(json) as Idl;
    const instructionMatch: Record<string, boolean> = {};
    for (const [name, expected] of Object.entries(EXPECTED_IX)) {
      const ix = (idl.instructions ?? []).find((row) => row.name === name);
      instructionMatch[name] = sameBytes(ix?.discriminator, expected);
    }
    return {
      found: true,
      address: address.toBase58(),
      instructionNames: (idl.instructions ?? []).map((ix) => ix.name),
      accountNames: (idl.accounts ?? []).map((account) => account.name),
      instructionMatch,
      error: null,
    };
  } catch (error) {
    return {
      ...empty,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

function unwrapAccount(account: object | null): object | null {
  if (!account) return null;
  if ("poolState" in account && account.poolState && typeof account.poolState === "object") {
    return account.poolState as object;
  }
  return account;
}

function layoutFields(account: object | null, names: readonly string[]) {
  const inner = unwrapAccount(account);
  if (!inner) {
    return { present: [] as string[], missing: [...names] };
  }
  const present = names.filter((name) => name in inner);
  return { present, missing: names.filter((name) => !present.includes(name)) };
}

async function decodeLiveAccounts(
  client: DynamicBondingCurveClient,
  connection: Connection,
  programId: PublicKey,
  idl: ReturnType<typeof extractSdkIdl>,
) {
  const decoded = {
    configsDecoded: 0,
    poolsDecoded: 0,
    sampleConfig: null as string | null,
    samplePool: null as string | null,
    configLayout: { present: [] as string[], missing: [] as string[] },
    poolLayout: { present: [] as string[], missing: [] as string[] },
    method: null as string | null,
    error: null as string | null,
  };

  try {
    const configs = await client.state.getPoolConfigs();
    decoded.configsDecoded = configs.length;
    decoded.method = "state.getPoolConfigs";
    if (configs[0]) {
      decoded.sampleConfig = configs[0].publicKey.toBase58();
      decoded.configLayout = layoutFields(configs[0].account as object, POOL_CONFIG_FIELDS);
    }
  } catch (error) {
    decoded.error = error instanceof Error ? error.message : String(error);
  }

  try {
    const pools = await client.state.getPools();
    decoded.poolsDecoded = pools.length;
    if (pools[0]) {
      decoded.samplePool = pools[0].publicKey.toBase58();
      decoded.poolLayout = layoutFields(pools[0].account as object, VIRTUAL_POOL_FIELDS);
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    decoded.error = decoded.error ? `${decoded.error}; pools: ${message}` : message;
  }

  if (decoded.configsDecoded > 0 || decoded.poolsDecoded > 0) {
    return decoded;
  }

  try {
    const configDisc = idl.accounts.PoolConfig;
    const accounts = await connection.getProgramAccounts(programId, {
      filters: [{ memcmp: { offset: 0, bytes: encodeBase58(configDisc) } }],
      dataSlice: { offset: 0, length: 0 },
    });
    decoded.method = "getProgramAccounts(PoolConfig discriminator)";
    decoded.configsDecoded = accounts.length;
    if (accounts[0]) {
      const config = await client.state.getPoolConfig(accounts[0].pubkey);
      decoded.sampleConfig = accounts[0].pubkey.toBase58();
      decoded.configLayout = layoutFields(config as object | null, POOL_CONFIG_FIELDS);
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    decoded.error = decoded.error ? `${decoded.error}; gpa: ${message}` : message;
  }

  if (decoded.configsDecoded > 0) {
    return decoded;
  }

  try {
    const sigs = await connection.getSignaturesForAddress(programId, { limit: 15 });
    decoded.method = "getSignaturesForAddress + getPoolConfig/getPool";
    for (const sig of sigs) {
      const tx = await connection.getTransaction(sig.signature, {
        maxSupportedTransactionVersion: 0,
      });
      const keys =
        tx?.transaction.message.getAccountKeys().staticAccountKeys ??
        [];
      for (const key of keys) {
        if (key.equals(programId)) continue;
        const [config, pool] = await Promise.all([
          client.state.getPoolConfig(key).catch(() => null),
          client.state.getPool(key).catch(() => null),
        ]);
        if (config && !decoded.sampleConfig) {
          decoded.sampleConfig = key.toBase58();
          decoded.configsDecoded += 1;
          decoded.configLayout = layoutFields(config as object, POOL_CONFIG_FIELDS);
        }
        if (pool && !decoded.samplePool) {
          decoded.samplePool = key.toBase58();
          decoded.poolsDecoded += 1;
          decoded.poolLayout = layoutFields(pool as object, VIRTUAL_POOL_FIELDS);
        }
        if (decoded.sampleConfig && decoded.samplePool) {
          return decoded;
        }
      }
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    decoded.error = decoded.error ? `${decoded.error}; sigs: ${message}` : message;
  }

  return decoded;
}

export function localSdkDiscriminatorReport() {
  const dbcVersion = installedPackageVersion("@meteora-ag/dynamic-bonding-curve-sdk");
  const cpVersion = installedPackageVersion("@meteora-ag/cp-amm-sdk");
  const idl = extractSdkIdl();
  const ixMatch: Record<string, { expected: number[]; sdk: number[] | null; ok: boolean; sha256: boolean }> =
    {};
  for (const [name, expected] of Object.entries(EXPECTED_IX)) {
    const sdk = idl.instructions[name] ?? null;
    ixMatch[name] = {
      expected,
      sdk,
      ok: sameBytes(sdk ?? undefined, expected),
      sha256: sameBytes(sdk ?? undefined, anchorDisc("global", name)),
    };
  }
  const accountMatch: Record<string, { expected: number[]; sdk: number[] | null; ok: boolean }> =
    {};
  for (const [name, expected] of Object.entries(EXPECTED_ACCOUNTS)) {
    const sdk = idl.accounts[name] ?? null;
    accountMatch[name] = {
      expected,
      sdk,
      ok: sameBytes(sdk ?? undefined, expected),
    };
  }
  return {
    dbcVersion,
    cpVersion,
    idlAddress: idl.address,
    metadataVersion: idl.metadataVersion,
    hasCreateConfig2: idl.instructionNames.includes("create_config2"),
    hasInitializeVirtualPool: idl.instructionNames.includes(
      "initialize_virtual_pool_with_spl_token",
    ),
    instructionCount: idl.instructionNames.length,
    accountCount: idl.accountNames.length,
    instructions: ixMatch,
    accounts: accountMatch,
    cpAmmProgramId: CP_AMM_PROGRAM_ID.toBase58(),
  };
}

export async function verifyDbcSdkAgainstDevnet() {
  const local = localSdkDiscriminatorReport();
  const connection = new Connection(RPC, "confirmed");
  const dbcPk = new PublicKey(DBC_PROGRAM);
  const dammPk = new PublicKey(DAMM_V2_PROGRAM);
  const [dbc, damm] = await Promise.all([
    connection.getAccountInfo(dbcPk),
    connection.getAccountInfo(dammPk),
  ]);

  const client = new DynamicBondingCurveClient(connection, "confirmed");
  const idl = extractSdkIdl(connection);
  const onChainIdl = await fetchOnChainIdl(connection, dbcPk);
  const live = await decodeLiveAccounts(client, connection, dbcPk, idl);

  let cpAmmClientOk = false;
  let cpAmmError: string | null = null;
  try {
    const cp = new CpAmm(connection);
    cpAmmClientOk = cp._program.programId.equals(dammPk);
  } catch (error) {
    cpAmmError = error instanceof Error ? error.message : String(error);
  }

  const discsOk = Object.values(local.instructions).every((row) => row.ok);
  const accountsOk = Object.values(local.accounts).every((row) => row.ok);
  const programOk = Boolean(dbc?.executable);
  const layoutOk =
    (live.configsDecoded > 0 && live.configLayout.missing.length === 0) ||
    (live.poolsDecoded > 0 && live.poolLayout.missing.length === 0);
  const onChainIdlOk =
    !onChainIdl.found ||
    Object.values(onChainIdl.instructionMatch).every(Boolean);

  const sdkMatchesDevnet =
    programOk &&
    discsOk &&
    accountsOk &&
    onChainIdlOk &&
    (layoutOk || live.error === null);

  return {
    rpc: RPC,
    sdk: `@meteora-ag/dynamic-bonding-curve-sdk@${local.dbcVersion}`,
    cpAmmSdk: `@meteora-ag/cp-amm-sdk@${local.cpVersion}`,
    dbcProgram: {
      id: DBC_PROGRAM,
      idlAddress: local.idlAddress ?? null,
      deployed: programOk,
      owner: dbc?.owner.toBase58() ?? null,
      dataLen: dbc?.data.length ?? 0,
      lamports: dbc?.lamports ?? 0,
    },
    dammV2Program: {
      id: DAMM_V2_PROGRAM,
      sdkProgramId: local.cpAmmProgramId,
      deployed: Boolean(damm?.executable),
      owner: damm?.owner.toBase58() ?? null,
      clientConstructs: cpAmmClientOk,
      error: cpAmmError,
    },
    idl: {
      metadataVersion: local.metadataVersion,
      instructionCount: local.instructionCount,
      accountCount: local.accountCount,
      hasCreateConfig2: local.hasCreateConfig2,
      hasInitializeVirtualPool: local.hasInitializeVirtualPool,
    },
    discriminators: local.instructions,
    accountDiscriminators: local.accounts,
    onChainIdl,
    liveAccountDecode: live,
    sdkMatchesDevnet,
  };
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const report = await verifyDbcSdkAgainstDevnet();
  console.log(JSON.stringify(report, null, 2));
  if (!report.sdkMatchesDevnet || !report.dbcProgram.deployed) {
    process.exitCode = 2;
  }
}
