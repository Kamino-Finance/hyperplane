import {Connection, Keypair, PublicKey, sendAndConfirmTransaction, SystemProgram, Transaction} from '@solana/web3.js';
import {
  approve,
  createAccount as createTokenAccount,
  createApproveInstruction,
  createInitializeAccountInstruction,
  createMint,
  getAccount as getTokenAccount,
  getAccountLenForMint,
  getMint,
  mintTo,
  TOKEN_PROGRAM_ID
} from '@solana/spl-token';

import {Numberu64, TOKEN_SWAP_PROGRAM_ID, TokenSwap} from '../src';
import {newAccountWithLamports} from '../src/util/new-account-with-lamports';
import {sleep} from '../src/util/sleep';
import {TOKEN_2022_PROGRAM_ID} from "@solana/spl-token";

// The following globals are created by `createTokenSwap` and used by subsequent tests
// Token swap
let tokenSwap: TokenSwap;
// owner of the user accounts
let owner: Keypair;
// Token pool
let adminAuthorityPoolTokenAta: PublicKey;
// Tokens swapped
let adminAuthorityTokenAAta: PublicKey;
let adminAuthorityTokenBAta: PublicKey;

// Hard-coded fee address, for testing production mode
const SWAP_PROGRAM_OWNER_FEE_ADDRESS =
  process.env.SWAP_PROGRAM_OWNER_FEE_ADDRESS;

// Pool fees
const TRADING_FEE_NUMERATOR = 25;
const TRADING_FEE_DENOMINATOR = 10000;
const OWNER_TRADING_FEE_NUMERATOR = 5;
const OWNER_TRADING_FEE_DENOMINATOR = 10000;
const OWNER_WITHDRAW_FEE_NUMERATOR = SWAP_PROGRAM_OWNER_FEE_ADDRESS ? 0 : 1;
const OWNER_WITHDRAW_FEE_DENOMINATOR = SWAP_PROGRAM_OWNER_FEE_ADDRESS ? 0 : 6;
const HOST_FEE_NUMERATOR = 20;
const HOST_FEE_DENOMINATOR = 100;

// Initial amount in each swap token
let currentSwapTokenA = 1000000;
let currentSwapTokenB = 1000000;
let currentFeeAmount = 0;

// Swap instruction constants
// Because there is no withdraw fee in the production version, these numbers
// need to get slightly tweaked in the two cases.
const SWAP_AMOUNT_IN = 100000;
const SWAP_AMOUNT_OUT = SWAP_PROGRAM_OWNER_FEE_ADDRESS ? 90661 : 90674;
const SWAP_FEE = SWAP_PROGRAM_OWNER_FEE_ADDRESS ? 22727 : 22730;
const HOST_SWAP_FEE = SWAP_PROGRAM_OWNER_FEE_ADDRESS
  ? Math.floor((SWAP_FEE * HOST_FEE_NUMERATOR) / HOST_FEE_DENOMINATOR)
  : 0;
const OWNER_SWAP_FEE = SWAP_FEE - HOST_SWAP_FEE;

// Pool token amount minted on init
const DEFAULT_POOL_TOKEN_AMOUNT = 1000000000;
// Pool token amount to withdraw / deposit
const POOL_TOKEN_AMOUNT = 10000000;

function assert(condition: boolean, message?: string) {
  if (!condition) {
    console.log(Error().stack + ':token-test.js');
    throw message || 'Assertion failed';
  }
}

let connection: Connection;
async function getConnection(): Promise<Connection> {
  if (connection) return connection;

  const url = 'http://localhost:8899';
  connection = new Connection(url, 'recent');
  const version = await connection.getVersion();

  console.log('Connection to cluster established:', url, version);
  return connection;
}

export async function createTokenSwap(
  curveType: number,
  curveParameters?: Numberu64,
): Promise<void> {
  const connection = await getConnection();
  const payer = await newAccountWithLamports(connection, 10_000000000);
  owner = await newAccountWithLamports(connection, 10_000000000);

  console.log('creating token A');
  const mintA = await createMint(
    connection,
    payer,
    owner.publicKey,
    null,
    2,
  );
  console.log('creating token A account');
  adminAuthorityTokenAAta = await createTokenAccount(
    connection,
    payer,
    mintA,
    owner.publicKey
  );
  console.log('minting token A to swap');
  await mintTo(connection, owner, mintA, adminAuthorityTokenAAta, owner.publicKey, currentSwapTokenA);

  console.log('creating token B');
  const mintB = await createMint(
    connection,
    payer,
    owner.publicKey,
    null,
    2,
  );
  console.log('creating token B account');
  adminAuthorityTokenBAta = await createTokenAccount(
    connection,
    payer,
    mintB,
    owner.publicKey
  );
  console.log('minting token B to swap');
  await mintTo(connection, owner, mintB, adminAuthorityTokenBAta, owner.publicKey, currentSwapTokenB);

  console.log('creating token swap');
  [tokenSwap, adminAuthorityPoolTokenAta] = await TokenSwap.createTokenSwap(
    connection,
    owner,
    adminAuthorityTokenAAta,
    adminAuthorityTokenBAta,
    mintA,
    mintB,
    TOKEN_2022_PROGRAM_ID,
    TRADING_FEE_NUMERATOR,
    TRADING_FEE_DENOMINATOR,
    OWNER_TRADING_FEE_NUMERATOR,
    OWNER_TRADING_FEE_DENOMINATOR,
    OWNER_WITHDRAW_FEE_NUMERATOR,
    OWNER_WITHDRAW_FEE_DENOMINATOR,
    HOST_FEE_NUMERATOR,
    HOST_FEE_DENOMINATOR,
    curveType,
    currentSwapTokenA,
    currentSwapTokenB,
    curveParameters,
  );

  console.log('loading token swap');
  const fetchedTokenSwap = await TokenSwap.loadTokenSwap(
    connection,
    tokenSwap.pool,
    owner,
  );

  assert(fetchedTokenSwap.poolTokenProgramId.equals(TOKEN_2022_PROGRAM_ID));
  assert(fetchedTokenSwap.tokenAVault.equals(tokenSwap.tokenAVault));
  assert(fetchedTokenSwap.tokenBVault.equals(tokenSwap.tokenBVault));
  assert(fetchedTokenSwap.mintA.equals(mintA));
  assert(fetchedTokenSwap.mintB.equals(mintB));
  assert(fetchedTokenSwap.poolToken.equals(tokenSwap.poolToken));
  assert(fetchedTokenSwap.feeAccount.equals(tokenSwap.feeAccount));
  assert(
    TRADING_FEE_NUMERATOR == fetchedTokenSwap.tradeFeeNumerator.toNumber(),
  );
  assert(
    TRADING_FEE_DENOMINATOR == fetchedTokenSwap.tradeFeeDenominator.toNumber(),
  );
  assert(
    OWNER_TRADING_FEE_NUMERATOR ==
      fetchedTokenSwap.ownerTradeFeeNumerator.toNumber(),
  );
  assert(
    OWNER_TRADING_FEE_DENOMINATOR ==
      fetchedTokenSwap.ownerTradeFeeDenominator.toNumber(),
  );
  assert(
    OWNER_WITHDRAW_FEE_NUMERATOR ==
      fetchedTokenSwap.ownerWithdrawFeeNumerator.toNumber(),
  );
  assert(
    OWNER_WITHDRAW_FEE_DENOMINATOR ==
      fetchedTokenSwap.ownerWithdrawFeeDenominator.toNumber(),
  );
  assert(HOST_FEE_NUMERATOR == fetchedTokenSwap.hostFeeNumerator.toNumber());
  assert(
    HOST_FEE_DENOMINATOR == fetchedTokenSwap.hostFeeDenominator.toNumber(),
  );
  assert(curveType == fetchedTokenSwap.curveType);
}

export async function depositAllTokenTypes(): Promise<void> {
  const poolMintInfo = await getMint(connection, tokenSwap.poolToken, undefined, TOKEN_2022_PROGRAM_ID);
  const supply = Number(poolMintInfo.supply);
  const swapTokenA = await getTokenAccount(connection, tokenSwap.tokenAVault);
  const tokenA = Math.floor(
    (Number(swapTokenA.amount) * POOL_TOKEN_AMOUNT) / supply,
  );
  const swapTokenB = await getTokenAccount(connection, tokenSwap.tokenBVault);
  const tokenB = Math.floor(
    (Number(swapTokenB.amount) * POOL_TOKEN_AMOUNT) / supply,
  );

  const userTransferAuthority = new Keypair();
  console.log('Creating depositor token a account');
  const userAccountA = await createTokenAccount(
    connection,
    owner,
    tokenSwap.mintA,
    owner.publicKey,
    new Keypair() // not ata
  );
  await mintTo(connection, owner, tokenSwap.mintA, userAccountA, owner, tokenA);

  await approve(
    connection,
    owner,
    userAccountA,
    userTransferAuthority.publicKey,
    owner,
    tokenA,
  );
  console.log('Creating depositor token b account');
  const userAccountB = await createTokenAccount(
    connection,
    owner,
    tokenSwap.mintB,
    owner.publicKey,
    new Keypair() // not ata
  );
  await mintTo(connection, owner, tokenSwap.mintB, userAccountB, owner, tokenB);
  // todo - elliot - delegation
  // await approve(
  //   connection,
  //   owner,
  //   userAccountB,
  //   userTransferAuthority.publicKey,
  //   owner,
  //   tokenB,
  // );
  console.log('Creating depositor pool token account');
  const newAccountPool = await createTokenAccount(
    connection,
    owner,
    tokenSwap.poolToken,
    owner.publicKey,
    new Keypair(), // not ata
    undefined,
    TOKEN_2022_PROGRAM_ID
  );

  console.log('Depositing into swap');
  await tokenSwap.depositAllTokenTypes(
    userAccountA,
    userAccountB,
    newAccountPool,
    TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    owner,
    POOL_TOKEN_AMOUNT,
    tokenA,
    tokenB,
  );

  let info;
  info = await getTokenAccount(connection, userAccountA);
  assert(Number(info.amount) == 0);
  info = await getTokenAccount(connection, userAccountB);
  assert(Number(info.amount) == 0);
  info = await getTokenAccount(connection, adminAuthorityTokenAAta);
  assert(Number(info.amount) == 0);
  info = await getTokenAccount(connection, adminAuthorityTokenBAta);
  assert(Number(info.amount) == 0);
  info = await getTokenAccount(connection, tokenSwap.tokenAVault);
  assert(Number(info.amount) == currentSwapTokenA + tokenA);
  currentSwapTokenA += tokenA;
  info = await getTokenAccount(connection, tokenSwap.tokenBVault);
  assert(Number(info.amount) == currentSwapTokenB + tokenB);
  currentSwapTokenB += tokenB;
  info = await getTokenAccount(connection, newAccountPool, undefined, TOKEN_2022_PROGRAM_ID);
  assert(Number(info.amount) == POOL_TOKEN_AMOUNT);
}

export async function withdrawAllTokenTypes(): Promise<void> {
  const poolMintInfo = await getMint(connection, tokenSwap.poolToken, undefined, TOKEN_2022_PROGRAM_ID);
  const supply = Number(poolMintInfo.supply);
  let swapTokenA = await getTokenAccount(connection, tokenSwap.tokenAVault);
  let swapTokenB = await getTokenAccount(connection, tokenSwap.tokenBVault);
  let feeAmount = 0;
  if (OWNER_WITHDRAW_FEE_NUMERATOR !== 0) {
    feeAmount = Math.floor(
      (POOL_TOKEN_AMOUNT * OWNER_WITHDRAW_FEE_NUMERATOR) /
        OWNER_WITHDRAW_FEE_DENOMINATOR,
    );
  }
  const poolTokenAmount = POOL_TOKEN_AMOUNT - feeAmount;
  const tokenA = Math.floor(
    (Number(swapTokenA.amount) * poolTokenAmount) / supply,
  );
  const tokenB = Math.floor(
    (Number(swapTokenB.amount) * poolTokenAmount) / supply,
  );

  console.log('Creating withdraw token A account');
  let userAccountA = await createTokenAccount(
    connection,
    owner,
    tokenSwap.mintA,
    owner.publicKey,
    new Keypair() // not ata
  );
  console.log('Creating withdraw token B account');
  let userAccountB = await createTokenAccount(
    connection,
    owner,
    tokenSwap.mintB,
    owner.publicKey,
    new Keypair() // not ata
  );

  const userTransferAuthority = new Keypair();
  console.log('Approving withdrawal from pool account');
  await approve(
    connection,
    owner,
    adminAuthorityPoolTokenAta,
    userTransferAuthority.publicKey,
    owner,
    POOL_TOKEN_AMOUNT,
    [],
    undefined,
    TOKEN_2022_PROGRAM_ID
  );

  console.log('Withdrawing pool tokens for A and B tokens');
  await tokenSwap.withdrawAllTokenTypes(
    userAccountA,
    userAccountB,
    adminAuthorityPoolTokenAta,
    TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    userTransferAuthority,
    POOL_TOKEN_AMOUNT,
    tokenA,
    tokenB,
  );

  //const poolMintInfo = await tokenPool.getMintInfo();
  swapTokenA = await getTokenAccount(connection, tokenSwap.tokenAVault);
  swapTokenB = await getTokenAccount(connection, tokenSwap.tokenBVault);

  let info = await getTokenAccount(connection, adminAuthorityPoolTokenAta, undefined, TOKEN_2022_PROGRAM_ID);;
  assert(
    Number(info.amount) == DEFAULT_POOL_TOKEN_AMOUNT - POOL_TOKEN_AMOUNT,
  );
  assert(Number(swapTokenA.amount) == currentSwapTokenA - tokenA);
  currentSwapTokenA -= tokenA;
  assert(Number(swapTokenB.amount) == currentSwapTokenB - tokenB);
  currentSwapTokenB -= tokenB;
  info = await getTokenAccount(connection, userAccountA);
  assert(Number(info.amount) == tokenA);
  info = await getTokenAccount(connection, userAccountB);
  assert(Number(info.amount) == tokenB);
  info = await getTokenAccount(connection, tokenSwap.feeAccount, undefined, TOKEN_2022_PROGRAM_ID);
  assert(Number(info.amount) == feeAmount);
  currentFeeAmount = feeAmount;
}

export async function createAccountAndSwapAtomic(): Promise<void> {
  console.log('Creating swap token a account');
  let userAccountA = await createTokenAccount(
    connection,
    owner,
    tokenSwap.mintA,
    owner.publicKey,
    new Keypair() // not ata
  );
  await mintTo(connection, owner, tokenSwap.mintA, userAccountA, owner, SWAP_AMOUNT_IN);

  const mintState = await getMint(connection, tokenSwap.mintA);
  const space = getAccountLenForMint(mintState);
  const lamports = await connection.getMinimumBalanceForRentExemption(space);

  const newAccount = new Keypair();
  const transaction = new Transaction();
  transaction.add(
    SystemProgram.createAccount({
      fromPubkey: owner.publicKey,
      newAccountPubkey: newAccount.publicKey,
      lamports,
      space,
      programId: TOKEN_PROGRAM_ID,
    }),
  );

  transaction.add(
    createInitializeAccountInstruction(
      newAccount.publicKey,
      tokenSwap.mintB,
      owner.publicKey,
    ),
  );

  // todo - elliot - delegation
  // const userTransferAuthority = new Keypair();
  // transaction.add(
  //   createApproveInstruction(
  //     userAccountA,
  //     userTransferAuthority.publicKey,
  //     owner.publicKey,
  //     SWAP_AMOUNT_IN
  //   ),
  // );

  transaction.add(
    TokenSwap.swapInstruction(
      tokenSwap.pool,
      tokenSwap.curve,
      tokenSwap.authority,
      owner.publicKey,
      userAccountA,
      tokenSwap.tokenAVault,
      tokenSwap.tokenBVault,
      newAccount.publicKey,
      tokenSwap.poolToken,
      tokenSwap.feeAccount,
      null,
      tokenSwap.mintA,
      tokenSwap.mintB,
      TOKEN_SWAP_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      tokenSwap.poolTokenProgramId,
      SWAP_AMOUNT_IN,
      0,
    ),
  );

  // Send the instructions
  console.log('sending big instruction');
  await sendAndConfirmTransaction(
    connection,
    transaction,
    [owner, newAccount],
  );

  let info;
  info = await getTokenAccount(connection, tokenSwap.tokenAVault);
  currentSwapTokenA = Number(info.amount);
  info = await getTokenAccount(connection, tokenSwap.tokenBVault);
  currentSwapTokenB = Number(info.amount);
}

export async function swap(): Promise<void> {
  console.log('Creating swap token a account');
  let userAccountA = await createTokenAccount(
    connection,
    owner,
    tokenSwap.mintA,
    owner.publicKey,
    new Keypair() // not ata
  );
  await mintTo(connection, owner, tokenSwap.mintA, userAccountA, owner, SWAP_AMOUNT_IN);
  const userTransferAuthority = new Keypair();
  await approve(
    connection,
    owner,
    userAccountA,
    userTransferAuthority.publicKey,
    owner,
    SWAP_AMOUNT_IN,
  );
  console.log('Creating swap token b account');
  let userAccountB = await createTokenAccount(
    connection,
    owner,
    tokenSwap.mintB,
    owner.publicKey,
    new Keypair() // not ata
  );
  let poolAccount = SWAP_PROGRAM_OWNER_FEE_ADDRESS
    ? await createTokenAccount(connection, owner, tokenSwap.poolToken, owner.publicKey, new Keypair(), undefined, TOKEN_2022_PROGRAM_ID)
    : null;

  console.log('Swapping');
  await tokenSwap.swap(
    userAccountA,
    tokenSwap.tokenAVault,
    tokenSwap.tokenBVault,
    userAccountB,
    tokenSwap.mintA,
    tokenSwap.mintB,
    TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    poolAccount,
    userTransferAuthority,
    SWAP_AMOUNT_IN,
    SWAP_AMOUNT_OUT,
  );

  await sleep(500);

  let info;
  info = await getTokenAccount(connection, userAccountA);
  assert(Number(info.amount) == 0);

  info = await getTokenAccount(connection, userAccountB);
  assert(Number(info.amount) == SWAP_AMOUNT_OUT);

  info = await getTokenAccount(connection, tokenSwap.tokenAVault);
  assert(Number(info.amount) == currentSwapTokenA + SWAP_AMOUNT_IN);
  currentSwapTokenA += SWAP_AMOUNT_IN;

  info = await getTokenAccount(connection, tokenSwap.tokenBVault);
  assert(Number(info.amount) == currentSwapTokenB - SWAP_AMOUNT_OUT);
  currentSwapTokenB -= SWAP_AMOUNT_OUT;

  info = await getTokenAccount(connection, adminAuthorityPoolTokenAta, undefined, TOKEN_2022_PROGRAM_ID);
  assert(
    Number(info.amount) == DEFAULT_POOL_TOKEN_AMOUNT - POOL_TOKEN_AMOUNT,
  );

  info = await getTokenAccount(connection, tokenSwap.feeAccount, undefined, TOKEN_2022_PROGRAM_ID);
  assert(Number(info.amount) == currentFeeAmount + OWNER_SWAP_FEE);

  if (poolAccount != null) {
    info = await getTokenAccount(connection, poolAccount, undefined, TOKEN_2022_PROGRAM_ID);
    assert(Number(info.amount) == HOST_SWAP_FEE);
  }
}

function tradingTokensToPoolTokens(
  sourceAmount: number,
  swapSourceAmount: number,
  poolAmount: number,
): number {
  const tradingFee =
    (sourceAmount / 2) * (TRADING_FEE_NUMERATOR / TRADING_FEE_DENOMINATOR);
  const ownerTradingFee =
    (sourceAmount / 2) * (OWNER_TRADING_FEE_NUMERATOR / OWNER_TRADING_FEE_DENOMINATOR);
  const sourceAmountPostFee = sourceAmount - tradingFee - ownerTradingFee;
  const root = Math.sqrt(sourceAmountPostFee / swapSourceAmount + 1);
  return Math.floor(poolAmount * (root - 1));
}

export async function depositSingleTokenTypeExactAmountIn(): Promise<void> {
  // Pool token amount to deposit on one side
  const depositAmount = 10000;

  const poolMintInfo = await getMint(connection, tokenSwap.poolToken, undefined, TOKEN_2022_PROGRAM_ID);
  const supply = Number(poolMintInfo.supply);
  const swapTokenA = await getTokenAccount(connection, tokenSwap.tokenAVault);
  const poolTokenA = tradingTokensToPoolTokens(
    depositAmount,
    Number(swapTokenA.amount),
    supply,
  );
  const swapTokenB = await getTokenAccount(connection, tokenSwap.tokenBVault);
  const poolTokenB = tradingTokensToPoolTokens(
    depositAmount,
    Number(swapTokenB.amount),
    supply,
  );

  const userTransferAuthority = new Keypair();
  console.log('Creating depositor token a account');
  const userAccountA = await createTokenAccount(
    connection,
    owner,
    tokenSwap.mintA,
    owner.publicKey,
    new Keypair() // not ata
  );
  await mintTo(connection, owner, tokenSwap.mintA, userAccountA, owner, depositAmount);
  await approve(
    connection,
    owner,
    userAccountA,
    userTransferAuthority.publicKey,
    owner,
    depositAmount,
  );
  console.log('Creating depositor token b account');
  const userAccountB = await createTokenAccount(
    connection,
    owner,
    tokenSwap.mintB,
    owner.publicKey,
    new Keypair() // not ata
  );
  await mintTo(connection, owner, tokenSwap.mintB, userAccountB, owner, depositAmount);
  await approve(
    connection,
    owner,
    userAccountB,
    userTransferAuthority.publicKey,
    owner,
    depositAmount,
  );
  console.log('Creating depositor pool token account');
  const newAccountPool = await createTokenAccount(
    connection,
    owner,
    tokenSwap.poolToken,
    owner.publicKey,
    new Keypair(), // not ata
    undefined,
    TOKEN_2022_PROGRAM_ID
  );

  console.log('Depositing token A into swap');
  await tokenSwap.depositSingleTokenTypeExactAmountIn(
    userAccountA,
    newAccountPool,
    tokenSwap.mintA,
    TOKEN_PROGRAM_ID,
    userTransferAuthority,
    depositAmount,
    poolTokenA,
  );

  let info;
  info = await getTokenAccount(connection, userAccountA);
  assert(Number(info.amount) == 0);
  info = await getTokenAccount(connection, tokenSwap.tokenAVault);
  assert(Number(info.amount) == currentSwapTokenA + depositAmount);
  currentSwapTokenA += depositAmount;

  console.log('Depositing token B into swap');
  await tokenSwap.depositSingleTokenTypeExactAmountIn(
    userAccountB,
    newAccountPool,
    tokenSwap.mintB,
    TOKEN_PROGRAM_ID,
    userTransferAuthority,
    depositAmount,
    poolTokenB,
  );

  info = await getTokenAccount(connection, userAccountB);
  assert(Number(info.amount) == 0);
  info = await getTokenAccount(connection, tokenSwap.tokenBVault);
  assert(Number(info.amount) == currentSwapTokenB + depositAmount);
  currentSwapTokenB += depositAmount;
  info = await getTokenAccount(connection, newAccountPool, undefined, TOKEN_2022_PROGRAM_ID);
  assert(Number(info.amount) >= poolTokenA + poolTokenB);
}

export async function withdrawSingleTokenTypeExactAmountOut(): Promise<void> {
  // Pool token amount to withdraw on one side
  const withdrawAmount = 50000;
  const roundingAmount = 1.0001; // make math a little easier

  const poolMintInfo = await getMint(connection, tokenSwap.poolToken, undefined, TOKEN_2022_PROGRAM_ID);
  const supply = Number(poolMintInfo.supply);

  const swapTokenA = await getTokenAccount(connection, tokenSwap.tokenAVault);
  const swapTokenAPost = Number(swapTokenA.amount) - withdrawAmount;
  const poolTokenA = tradingTokensToPoolTokens(
    withdrawAmount,
    swapTokenAPost,
    supply,
  );
  let adjustedPoolTokenA = poolTokenA * roundingAmount;
  if (OWNER_WITHDRAW_FEE_NUMERATOR !== 0) {
    adjustedPoolTokenA *=
      1 + OWNER_WITHDRAW_FEE_NUMERATOR / OWNER_WITHDRAW_FEE_DENOMINATOR;
  }

  const swapTokenB = await getTokenAccount(connection, tokenSwap.tokenBVault);
  const swapTokenBPost = Number(swapTokenB.amount) - withdrawAmount;
  const poolTokenB = tradingTokensToPoolTokens(
    withdrawAmount,
    swapTokenBPost,
    supply,
  );
  let adjustedPoolTokenB = poolTokenB * roundingAmount;
  if (OWNER_WITHDRAW_FEE_NUMERATOR !== 0) {
    adjustedPoolTokenB *=
      1 + OWNER_WITHDRAW_FEE_NUMERATOR / OWNER_WITHDRAW_FEE_DENOMINATOR;
  }

  const userTransferAuthority = new Keypair();
  console.log('Creating withdraw token a account');
  const userAccountA = await createTokenAccount(
    connection,
    owner,
    tokenSwap.mintA,
    owner.publicKey,
    new Keypair() // not ata
  );
  console.log('Creating withdraw token b account');
  const userAccountB = await createTokenAccount(
    connection,
    owner,
    tokenSwap.mintB,
    owner.publicKey,
    new Keypair() // not ata
  );
  console.log('Creating withdraw pool token account');
  const poolAccount = await getTokenAccount(connection, adminAuthorityPoolTokenAta, undefined, TOKEN_2022_PROGRAM_ID);
  const poolTokenAmount = Number(poolAccount.amount);
  await approve(
    connection,
    owner,
    adminAuthorityPoolTokenAta,
    userTransferAuthority.publicKey,
    owner,
    Math.ceil(adjustedPoolTokenA + adjustedPoolTokenB),
    [],
    undefined,
    TOKEN_2022_PROGRAM_ID
  );

  console.log('Withdrawing token A only');
  await tokenSwap.withdrawSingleTokenTypeExactAmountOut(
    userAccountA,
    adminAuthorityPoolTokenAta,
    tokenSwap.mintA,
    TOKEN_PROGRAM_ID,
    userTransferAuthority,
    withdrawAmount,
    adjustedPoolTokenA,
  );

  let info;
  info = await getTokenAccount(connection, userAccountA);
  assert(Number(info.amount) == withdrawAmount);
  info = await getTokenAccount(connection, tokenSwap.tokenAVault);
  assert(Number(info.amount) == currentSwapTokenA - withdrawAmount);
  currentSwapTokenA += withdrawAmount;
  info = await getTokenAccount(connection, adminAuthorityPoolTokenAta, undefined, TOKEN_2022_PROGRAM_ID);
  assert(Number(info.amount) >= poolTokenAmount - adjustedPoolTokenA);

  console.log('Withdrawing token B only');
  await tokenSwap.withdrawSingleTokenTypeExactAmountOut(
    userAccountB,
    adminAuthorityPoolTokenAta,
    tokenSwap.mintB,
    TOKEN_PROGRAM_ID,
    userTransferAuthority,
    withdrawAmount,
    adjustedPoolTokenB,
  );

  info = await getTokenAccount(connection, userAccountB);
  assert(Number(info.amount) == withdrawAmount);
  info = await getTokenAccount(connection, tokenSwap.tokenBVault);
  assert(Number(info.amount) == currentSwapTokenB - withdrawAmount);
  currentSwapTokenB += withdrawAmount;
  info = await getTokenAccount(connection, adminAuthorityPoolTokenAta, undefined, TOKEN_2022_PROGRAM_ID);

  assert(
    Number(info.amount) >=
      poolTokenAmount - adjustedPoolTokenA - adjustedPoolTokenB,
  );
}
