import assert from 'assert';
import BN from 'bn.js';
import {Buffer} from 'buffer';
import type {
  ConfirmOptions,
  Connection,
  TransactionSignature,
} from '@solana/web3.js';
import {
  Keypair,
  PublicKey,
  sendAndConfirmTransaction,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  Transaction,
  TransactionInstruction,
} from '@solana/web3.js';
import * as Instructions from './_generated/hyperplane-client/instructions';
import * as Accounts from './_generated/hyperplane-client/accounts';

import {ConstantProduct} from './_generated/hyperplane-client/types/CurveType';
import {Fees} from './_generated/hyperplane-client/types';
import {PROGRAM_ID} from './_generated/hyperplane-client/programId';
import {TOKEN_PROGRAM_ID} from '@solana/spl-token';
import {SWAP_POOL_ACCOUNT_LEN} from './util/const';
import {
  ConstantPrice,
  Offset,
} from './_generated/hyperplane-client/types/CurveParameters';

export const TOKEN_SWAP_PROGRAM_ID = PROGRAM_ID;

/**
 * Some amount of tokens
 */
export class Numberu64 extends BN {
  /**
   * Convert to Buffer representation
   */
  toBuffer(): Buffer {
    const a = super.toArray().reverse();
    const b = Buffer.from(a);
    if (b.length === 8) {
      return b;
    }
    assert(b.length < 8, 'Numberu64 too large');

    const zeroPad = Buffer.alloc(8);
    b.copy(zeroPad);
    return zeroPad;
  }

  /**
   * Construct a Numberu64 from Buffer representation
   */
  static fromBuffer(buffer: Buffer): Numberu64 {
    assert(buffer.length === 8, `Invalid buffer length: ${buffer.length}`);
    return new Numberu64(
      [...buffer]
        .reverse()
        .map(i => `00${i.toString(16)}`.slice(-2))
        .join(''),
      16,
    );
  }
}

export const CurveType = Object.freeze({
  ConstantProduct: 1, // Constant product curve, Uniswap-style
  ConstantPrice: 2, // Constant price curve, always X amount of A token for 1 B token, where X is defined at init
  Offset: 3, // Offset curve, like Uniswap, but with an additional offset on the token B side
});

export function getCurveParams(curveType: number, params?: any) {
  switch (curveType) {
    case CurveType.ConstantProduct:
      return new ConstantProduct();
    case CurveType.ConstantPrice:
      return new ConstantPrice({tokenBPrice: params});
    case CurveType.Offset:
      return new Offset({tokenBOffset: params});
    default:
      throw new Error('invalid curve type');
  }
}

/**
 * A program to exchange tokens against a pool of liquidity
 */
export class TokenSwap {
  /**
   * Create a Token object attached to the specific token
   *
   * @param connection The connection to use
   * @param pool The token swap account
   * @param swapProgramId The program ID of the token-swap program
   * @param poolTokenProgramId The program ID of the token program for the pool tokens
   * @param poolToken The pool token
   * @param authority The authority over the swap and accounts
   * @param tokenAVault The token swap's Token A account
   * @param tokenBVault The token swap's Token B account
   * @param mintA The mint of Token A
   * @param mintB The mint of Token B
   * @param tradeFeeNumerator The trade fee numerator
   * @param tradeFeeDenominator The trade fee denominator
   * @param ownerTradeFeeNumerator The owner trade fee numerator
   * @param ownerTradeFeeDenominator The owner trade fee denominator
   * @param ownerWithdrawFeeNumerator The owner withdraw fee numerator
   * @param ownerWithdrawFeeDenominator The owner withdraw fee denominator
   * @param hostFeeNumerator The host fee numerator
   * @param hostFeeDenominator The host fee denominator
   * @param curveType The curve type
   * @param payer Pays for the transaction
   */
  constructor(
    private connection: Connection,
    public pool: PublicKey,
    public poolTokenProgramId: PublicKey,
    public poolToken: PublicKey,
    public feeAccount: PublicKey,
    public authority: PublicKey,
    public tokenAVault: PublicKey,
    public tokenBVault: PublicKey,
    public mintA: PublicKey,
    public mintB: PublicKey,
    public tradeFeeNumerator: Numberu64,
    public tradeFeeDenominator: Numberu64,
    public ownerTradeFeeNumerator: Numberu64,
    public ownerTradeFeeDenominator: Numberu64,
    public ownerWithdrawFeeNumerator: Numberu64,
    public ownerWithdrawFeeDenominator: Numberu64,
    public hostFeeNumerator: Numberu64,
    public hostFeeDenominator: Numberu64,
    public curveType: number,
    public curve: PublicKey,
    public payer: Keypair,
  ) {
    this.connection = connection;
    this.pool = pool;
    this.poolTokenProgramId = poolTokenProgramId;
    this.poolToken = poolToken;
    this.feeAccount = feeAccount;
    this.authority = authority;
    this.tokenAVault = tokenAVault;
    this.tokenBVault = tokenBVault;
    this.mintA = mintA;
    this.mintB = mintB;
    this.tradeFeeNumerator = tradeFeeNumerator;
    this.tradeFeeDenominator = tradeFeeDenominator;
    this.ownerTradeFeeNumerator = ownerTradeFeeNumerator;
    this.ownerTradeFeeDenominator = ownerTradeFeeDenominator;
    this.ownerWithdrawFeeNumerator = ownerWithdrawFeeNumerator;
    this.ownerWithdrawFeeDenominator = ownerWithdrawFeeDenominator;
    this.hostFeeNumerator = hostFeeNumerator;
    this.hostFeeDenominator = hostFeeDenominator;
    this.curveType = curveType;
    this.curve = curve;
    this.payer = payer;
  }

  static async loadTokenSwap(
    connection: Connection,
    address: PublicKey,
    payer: Keypair,
  ): Promise<TokenSwap> {
    const swapPool = await Accounts.SwapPool.fetch(connection, address);

    if (swapPool == null || !swapPool.isInitialized) {
      throw new Error(`Invalid token swap state: ${swapPool}`);
    }

    const poolMint = await connection.getAccountInfo(swapPool.poolTokenMint);
    if (!poolMint) {
      throw new Error(`Swap pool mint not found: ${swapPool.poolTokenMint}`);
    }

    return new TokenSwap(
      connection,
      address,
      poolMint?.owner,
      swapPool.poolTokenMint,
      swapPool.poolTokenFeesVault,
      swapPool.poolAuthority,
      swapPool.tokenAVault,
      swapPool.tokenBVault,
      swapPool.tokenAMint,
      swapPool.tokenBMint,
      swapPool.fees.tradeFeeNumerator,
      swapPool.fees.tradeFeeDenominator,
      swapPool.fees.ownerTradeFeeNumerator,
      swapPool.fees.ownerTradeFeeDenominator,
      swapPool.fees.ownerWithdrawFeeNumerator,
      swapPool.fees.ownerWithdrawFeeDenominator,
      swapPool.fees.hostFeeNumerator,
      swapPool.fees.hostFeeDenominator,
      swapPool.curveType.toNumber(),
      swapPool.swapCurve,
      payer,
    );
  }

  /**
   * Create a new Token Swap
   *
   * @param connection The connection to use
   * @param payer Pays for the transaction
   * @param tokenSwapAccount The token swap account
   * @param authority The authority over the swap and accounts
   * @param adminAuthorityTokenAAta: The funding Token A account
   * @param adminAuthorityTokenBAta: The funding Token B account
   * @param poolToken The pool token
   * @param tokenAccountPool The token swap's pool token account
   * @param poolTokenProgramId The program ID of the token program for pool tokens
   * @param swapProgramId The program ID of the token-swap program
   * @param feeNumerator Numerator of the fee ratio
   * @param feeDenominator Denominator of the fee ratio
   * @return Token object for the newly minted token, Public key of the account holding the total supply of new tokens
   */
  static async createTokenSwap(
    connection: Connection,
    payer: Keypair,
    adminAuthorityTokenAAta: PublicKey,
    adminAuthorityTokenBAta: PublicKey,
    mintA: PublicKey,
    mintB: PublicKey,
    poolTokenProgramId: PublicKey,
    tradeFeeNumerator: number,
    tradeFeeDenominator: number,
    ownerTradeFeeNumerator: number,
    ownerTradeFeeDenominator: number,
    ownerWithdrawFeeNumerator: number,
    ownerWithdrawFeeDenominator: number,
    hostFeeNumerator: number,
    hostFeeDenominator: number,
    curveType: number,
    initialSupplyA: number,
    initialSupplyB: number,
    curveParameters?: Numberu64,
    confirmOptions?: ConfirmOptions,
  ): Promise<[TokenSwap, PublicKey]> {
    const pool = new Keypair();
    const lamports = await connection.getMinimumBalanceForRentExemption(
      SWAP_POOL_ACCOUNT_LEN,
    );
    const reservePoolAccountSpaceIx = SystemProgram.createAccount({
      fromPubkey: payer.publicKey,
      newAccountPubkey: pool.publicKey,
      lamports,
      space: SWAP_POOL_ACCOUNT_LEN,
      programId: TOKEN_SWAP_PROGRAM_ID,
    });

    const [swapCurve, _swapCurveBump] = PublicKey.findProgramAddressSync(
      [Buffer.from('curve'), pool.publicKey.toBuffer()],
      TOKEN_SWAP_PROGRAM_ID,
    );

    const [poolAuthority, _poolAuthorityBump] =
      PublicKey.findProgramAddressSync(
        [Buffer.from('pauthority'), pool.publicKey.toBuffer()],
        TOKEN_SWAP_PROGRAM_ID,
      );

    const [poolTokenMint, _poolTokenMintBump] =
      PublicKey.findProgramAddressSync(
        [Buffer.from('lp'), pool.publicKey.toBuffer()],
        TOKEN_SWAP_PROGRAM_ID,
      );

    const [poolTokenFeesVault, _poolTokenFeesVaultBump] =
      PublicKey.findProgramAddressSync(
        [
          Buffer.from('lpfee'),
          pool.publicKey.toBuffer(),
          poolTokenMint.toBuffer(),
        ],
        TOKEN_SWAP_PROGRAM_ID,
      );

    const [tokenAVault, _tokenAVaultBump] = PublicKey.findProgramAddressSync(
      [Buffer.from('pvault_a'), pool.publicKey.toBuffer(), mintA.toBuffer()],
      TOKEN_SWAP_PROGRAM_ID,
    );
    const [tokenBVault, _tokenBVaultBump] = PublicKey.findProgramAddressSync(
      [Buffer.from('pvault_b'), pool.publicKey.toBuffer(), mintB.toBuffer()],
      TOKEN_SWAP_PROGRAM_ID,
    );

    const adminAuthorityPoolTokenAta = new Keypair();

    const ix = Instructions.initializePool(
      {
        curveParameters: getCurveParams(curveType, curveParameters),
        fees: new Fees({
          tradeFeeNumerator: new Numberu64(tradeFeeNumerator),
          tradeFeeDenominator: new Numberu64(tradeFeeDenominator),
          ownerTradeFeeNumerator: new Numberu64(ownerTradeFeeNumerator),
          ownerTradeFeeDenominator: new Numberu64(ownerTradeFeeDenominator),
          ownerWithdrawFeeNumerator: new Numberu64(ownerWithdrawFeeNumerator),
          ownerWithdrawFeeDenominator: new Numberu64(
            ownerWithdrawFeeDenominator,
          ),
          hostFeeNumerator: new Numberu64(hostFeeNumerator),
          hostFeeDenominator: new Numberu64(hostFeeDenominator),
        }),
        initialSupplyA: new Numberu64(initialSupplyA),
        initialSupplyB: new Numberu64(initialSupplyB),
      },
      {
        pool: pool.publicKey,
        swapCurve: swapCurve,
        adminAuthority: payer.publicKey,
        adminAuthorityPoolTokenAta: adminAuthorityPoolTokenAta.publicKey,
        adminAuthorityTokenAAta,
        adminAuthorityTokenBAta,
        poolAuthority,
        poolTokenFeesVault,
        poolTokenMint,
        tokenAMint: mintA,
        tokenAVault,
        tokenBMint: mintB,
        tokenBVault,
        poolTokenProgram: poolTokenProgramId,
        tokenATokenProgram: TOKEN_PROGRAM_ID,
        tokenBTokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      },
    );

    const tokenSwap = new TokenSwap(
      connection,
      pool.publicKey,
      poolTokenProgramId,
      poolTokenMint,
      poolTokenFeesVault,
      poolAuthority,
      tokenAVault,
      tokenBVault,
      mintA,
      mintB,
      new Numberu64(tradeFeeNumerator),
      new Numberu64(tradeFeeDenominator),
      new Numberu64(ownerTradeFeeNumerator),
      new Numberu64(ownerTradeFeeDenominator),
      new Numberu64(ownerWithdrawFeeNumerator),
      new Numberu64(ownerWithdrawFeeDenominator),
      new Numberu64(hostFeeNumerator),
      new Numberu64(hostFeeDenominator),
      curveType,
      swapCurve,
      payer,
    );

    const tx = new Transaction().add(reservePoolAccountSpaceIx).add(ix);
    await sendAndConfirmTransaction(
      connection,
      tx,
      [payer, pool, adminAuthorityPoolTokenAta],
      confirmOptions,
    );

    return [tokenSwap, adminAuthorityPoolTokenAta.publicKey];
  }

  /**
   * Swap token A for token B
   *
   * @param userSource User's source token account
   * @param poolSource Pool's source token account
   * @param poolDestination Pool's destination token account
   * @param userDestination User's destination token account
   * @param sourceMint Mint for the source token
   * @param destinationMint Mint for the destination token
   * @param sourceTokenProgramId Program id for the source token
   * @param destinationTokenProgramId Program id for the destination token
   * @param hostFeeAccount Host account to gather fees
   * @param userTransferAuthority Account delegated to transfer user's tokens
   * @param amountIn Amount to transfer from source account
   * @param minimumAmountOut Minimum amount of tokens the user will receive
   */
  async swap(
    userSource: PublicKey,
    poolSource: PublicKey,
    poolDestination: PublicKey,
    userDestination: PublicKey,
    sourceMint: PublicKey,
    destinationMint: PublicKey,
    sourceTokenProgramId: PublicKey,
    destinationTokenProgramId: PublicKey,
    hostFeeAccount: PublicKey | null,
    userTransferAuthority: Keypair,
    amountIn: number | Numberu64,
    minimumAmountOut: number | Numberu64,
    confirmOptions?: ConfirmOptions,
  ): Promise<TransactionSignature> {
    return await sendAndConfirmTransaction(
      this.connection,
      new Transaction().add(
        TokenSwap.swapInstruction(
          this.pool,
          this.curve,
          this.authority,
          // userTransferAuthority.publicKey,
          this.payer.publicKey, // todo - elliot - delegation
          userSource,
          poolSource,
          poolDestination,
          userDestination,
          this.poolToken,
          this.feeAccount,
          hostFeeAccount,
          sourceMint,
          destinationMint,
          TOKEN_SWAP_PROGRAM_ID,
          sourceTokenProgramId,
          destinationTokenProgramId,
          this.poolTokenProgramId,
          amountIn,
          minimumAmountOut,
        ),
      ),
      [this.payer],
      confirmOptions,
    );
  }

  static swapInstruction(
    pool: PublicKey,
    curve: PublicKey,
    authority: PublicKey,
    userTransferAuthority: PublicKey,
    userSource: PublicKey,
    poolSource: PublicKey,
    poolDestination: PublicKey,
    userDestination: PublicKey,
    poolMint: PublicKey,
    feeAccount: PublicKey,
    hostFeeAccount: PublicKey | null,
    sourceMint: PublicKey,
    destinationMint: PublicKey,
    swapProgramId: PublicKey,
    sourceTokenProgramId: PublicKey,
    destinationTokenProgramId: PublicKey,
    poolTokenProgramId: PublicKey,
    amountIn: number | Numberu64,
    minimumAmountOut: number | Numberu64,
  ): TransactionInstruction {
    return Instructions.swap(
      {
        amountIn: new Numberu64(amountIn),
        minimumAmountOut: new Numberu64(minimumAmountOut),
      },
      {
        signer: userTransferAuthority,
        pool: pool,
        swapCurve: curve,
        poolAuthority: authority,
        sourceMint,
        destinationMint,
        sourceVault: poolSource,
        destinationVault: poolDestination,
        poolTokenMint: poolMint,
        poolTokenFeesVault: feeAccount,
        sourceUserAta: userSource,
        destinationUserAta: userDestination,
        poolTokenHostFeesAccount: hostFeeAccount || TOKEN_SWAP_PROGRAM_ID,
        poolTokenProgram: poolTokenProgramId,
        sourceTokenProgram: TOKEN_PROGRAM_ID,
        destinationTokenProgram: TOKEN_PROGRAM_ID,
      },
    );
  }

  /**
   * Deposit tokens into the pool
   * @param userAccountA User account for token A
   * @param userAccountB User account for token B
   * @param poolAccount User account for pool token
   * @param tokenProgramIdA Program id for token A
   * @param tokenProgramIdB Program id for token B
   * @param userTransferAuthority Account delegated to transfer user's tokens
   * @param poolTokenAmount Amount of pool tokens to mint
   * @param maximumTokenA The maximum amount of token A to deposit
   * @param maximumTokenB The maximum amount of token B to deposit
   */
  async depositAllTokenTypes(
    userAccountA: PublicKey,
    userAccountB: PublicKey,
    poolAccount: PublicKey,
    tokenProgramIdA: PublicKey,
    tokenProgramIdB: PublicKey,
    userTransferAuthority: Keypair,
    poolTokenAmount: number | Numberu64,
    maximumTokenA: number | Numberu64,
    maximumTokenB: number | Numberu64,
    confirmOptions?: ConfirmOptions,
  ): Promise<TransactionSignature> {
    return await sendAndConfirmTransaction(
      this.connection,
      new Transaction().add(
        TokenSwap.depositAllTokenTypesInstruction(
          this.pool,
          this.curve,
          this.authority,
          userTransferAuthority.publicKey,
          userAccountA,
          userAccountB,
          this.tokenAVault,
          this.tokenBVault,
          this.poolToken,
          poolAccount,
          this.mintA,
          this.mintB,
          TOKEN_SWAP_PROGRAM_ID,
          tokenProgramIdA,
          tokenProgramIdB,
          this.poolTokenProgramId,
          poolTokenAmount,
          maximumTokenA,
          maximumTokenB,
        ),
      ),
      [userTransferAuthority],
      confirmOptions,
    );
  }

  static depositAllTokenTypesInstruction(
    pool: PublicKey,
    curve: PublicKey,
    authority: PublicKey,
    userTransferAuthority: PublicKey,
    sourceA: PublicKey,
    sourceB: PublicKey,
    intoA: PublicKey,
    intoB: PublicKey,
    poolToken: PublicKey,
    poolAccount: PublicKey,
    mintA: PublicKey,
    mintB: PublicKey,
    swapProgramId: PublicKey,
    tokenProgramIdA: PublicKey,
    tokenProgramIdB: PublicKey,
    poolTokenProgramId: PublicKey,
    poolTokenAmount: number | Numberu64,
    maximumTokenA: number | Numberu64,
    maximumTokenB: number | Numberu64,
  ): TransactionInstruction {
    return Instructions.depositAllTokenTypes(
      {
        poolTokenAmount: new Numberu64(poolTokenAmount),
        maximumTokenAAmount: new Numberu64(maximumTokenA),
        maximumTokenBAmount: new Numberu64(maximumTokenB),
      },
      {
        signer: userTransferAuthority,
        pool,
        swapCurve: curve,
        poolAuthority: authority,
        tokenAMint: mintA,
        tokenBMint: mintB,
        tokenAVault: intoA,
        tokenBVault: intoB,
        poolTokenMint: poolToken,
        tokenAUserAta: sourceA,
        tokenBUserAta: sourceB,
        poolTokenUserAta: poolAccount,
        poolTokenProgram: poolTokenProgramId,
        tokenATokenProgram: tokenProgramIdA,
        tokenBTokenProgram: tokenProgramIdB,
      },
    );
  }

  /**
   * Withdraw tokens from the pool
   *
   * @param userAccountA User account for token A
   * @param userAccountB User account for token B
   * @param poolAccount User account for pool token
   * @param tokenProgramIdA Program id for token A
   * @param tokenProgramIdB Program id for token B
   * @param userTransferAuthority Account delegated to transfer user's tokens
   * @param poolTokenAmount Amount of pool tokens to burn
   * @param minimumTokenA The minimum amount of token A to withdraw
   * @param minimumTokenB The minimum amount of token B to withdraw
   */
  async withdrawAllTokenTypes(
    userAccountA: PublicKey,
    userAccountB: PublicKey,
    poolAccount: PublicKey,
    tokenProgramIdA: PublicKey,
    tokenProgramIdB: PublicKey,
    userTransferAuthority: Keypair,
    poolTokenAmount: number | Numberu64,
    minimumTokenA: number | Numberu64,
    minimumTokenB: number | Numberu64,
    confirmOptions?: ConfirmOptions,
  ): Promise<TransactionSignature> {
    return await sendAndConfirmTransaction(
      this.connection,
      new Transaction().add(
        TokenSwap.withdrawAllTokenTypesInstruction(
          this.pool,
          this.curve,
          this.authority,
          userTransferAuthority.publicKey,
          this.poolToken,
          this.feeAccount,
          poolAccount,
          this.tokenAVault,
          this.tokenBVault,
          userAccountA,
          userAccountB,
          this.mintA,
          this.mintB,
          TOKEN_SWAP_PROGRAM_ID,
          this.poolTokenProgramId,
          tokenProgramIdA,
          tokenProgramIdB,
          poolTokenAmount,
          minimumTokenA,
          minimumTokenB,
        ),
      ),
      [userTransferAuthority],
      confirmOptions,
    );
  }

  static withdrawAllTokenTypesInstruction(
    pool: PublicKey,
    curve: PublicKey,
    authority: PublicKey,
    userTransferAuthority: PublicKey,
    poolMint: PublicKey,
    feeAccount: PublicKey,
    sourcePoolAccount: PublicKey,
    fromA: PublicKey,
    fromB: PublicKey,
    userAccountA: PublicKey,
    userAccountB: PublicKey,
    mintA: PublicKey,
    mintB: PublicKey,
    swapProgramId: PublicKey,
    poolTokenProgramId: PublicKey,
    tokenProgramIdA: PublicKey,
    tokenProgramIdB: PublicKey,
    poolTokenAmount: number | Numberu64,
    minimumTokenA: number | Numberu64,
    minimumTokenB: number | Numberu64,
  ): TransactionInstruction {
    return Instructions.withdrawAllTokenTypes(
      {
        poolTokenAmount: new Numberu64(poolTokenAmount),
        minimumTokenAAmount: new Numberu64(minimumTokenA),
        minimumTokenBAmount: new Numberu64(minimumTokenB),
      },
      {
        signer: userTransferAuthority,
        pool,
        swapCurve: curve,
        poolAuthority: authority,
        tokenAMint: mintA,
        tokenBMint: mintB,
        tokenAVault: fromA,
        tokenBVault: fromB,
        poolTokenMint: poolMint,
        poolTokenFeesVault: feeAccount,
        tokenAUserAta: userAccountA,
        tokenBUserAta: userAccountB,
        poolTokenUserAta: sourcePoolAccount,
        poolTokenProgram: poolTokenProgramId,
        tokenATokenProgram: tokenProgramIdA,
        tokenBTokenProgram: tokenProgramIdB,
      },
    );
  }

  /**
   * Deposit one side of tokens into the pool
   * @param userAccount User account to deposit token A or B
   * @param poolAccount User account to receive pool tokens
   * @param sourceMint Mint for the source token
   * @param sourceTokenProgramId Program id for the source token
   * @param userTransferAuthority Account delegated to transfer user's tokens
   * @param sourceTokenAmount The amount of token A or B to deposit
   * @param minimumPoolTokenAmount Minimum amount of pool tokens to mint
   */
  async depositSingleTokenTypeExactAmountIn(
    userAccount: PublicKey,
    poolAccount: PublicKey,
    sourceMint: PublicKey,
    sourceTokenProgramId: PublicKey,
    userTransferAuthority: Keypair,
    sourceTokenAmount: number | Numberu64,
    minimumPoolTokenAmount: number | Numberu64,
    confirmOptions?: ConfirmOptions,
  ): Promise<TransactionSignature> {
    return await sendAndConfirmTransaction(
      this.connection,
      new Transaction().add(
        TokenSwap.depositSingleTokenTypeExactAmountInInstruction(
          this.pool,
          this.curve,
          this.authority,
          userTransferAuthority.publicKey,
          userAccount,
          this.tokenAVault,
          this.tokenBVault,
          this.poolToken,
          poolAccount,
          sourceMint,
          TOKEN_SWAP_PROGRAM_ID,
          sourceTokenProgramId,
          this.poolTokenProgramId,
          sourceTokenAmount,
          minimumPoolTokenAmount,
        ),
      ),
      [this.payer],
      confirmOptions,
    );
  }

  static depositSingleTokenTypeExactAmountInInstruction(
    pool: PublicKey,
    curve: PublicKey,
    authority: PublicKey,
    userTransferAuthority: PublicKey,
    source: PublicKey,
    intoA: PublicKey,
    intoB: PublicKey,
    poolToken: PublicKey,
    poolAccount: PublicKey,
    sourceMint: PublicKey,
    swapProgramId: PublicKey,
    sourceTokenProgramId: PublicKey,
    poolTokenProgramId: PublicKey,
    sourceTokenAmount: number | Numberu64,
    minimumPoolTokenAmount: number | Numberu64,
  ): TransactionInstruction {
    return Instructions.depositSingleTokenType(
      {
        sourceTokenAmount: new Numberu64(sourceTokenAmount),
        minimumPoolTokenAmount: new Numberu64(minimumPoolTokenAmount),
      },
      {
        signer: userTransferAuthority,
        pool,
        swapCurve: curve,
        poolAuthority: authority,
        sourceTokenMint: sourceMint,
        tokenAVault: intoA,
        tokenBVault: intoB,
        poolTokenMint: poolToken,
        sourceTokenUserAta: source,
        poolTokenUserAta: poolAccount,
        poolTokenProgram: poolTokenProgramId,
        sourceTokenProgram: sourceTokenProgramId,
      },
    );
  }

  /**
   * Withdraw tokens from the pool
   *
   * @param userAccount User account to receive token A or B
   * @param poolAccount User account to burn pool token
   * @param destinationMint Mint for the destination token
   * @param destinationTokenProgramId Program id for the destination token
   * @param userTransferAuthority Account delegated to transfer user's tokens
   * @param destinationTokenAmount The amount of token A or B to withdraw
   * @param maximumPoolTokenAmount Maximum amount of pool tokens to burn
   */
  async withdrawSingleTokenTypeExactAmountOut(
    userAccount: PublicKey,
    poolAccount: PublicKey,
    destinationMint: PublicKey,
    destinationTokenProgramId: PublicKey,
    userTransferAuthority: Keypair,
    destinationTokenAmount: number | Numberu64,
    maximumPoolTokenAmount: number | Numberu64,
    confirmOptions?: ConfirmOptions,
  ): Promise<TransactionSignature> {
    return await sendAndConfirmTransaction(
      this.connection,
      new Transaction().add(
        TokenSwap.withdrawSingleTokenTypeExactAmountOutInstruction(
          this.pool,
          this.curve,
          this.authority,
          userTransferAuthority.publicKey,
          this.poolToken,
          this.feeAccount,
          poolAccount,
          this.tokenAVault,
          this.tokenBVault,
          userAccount,
          destinationMint,
          TOKEN_SWAP_PROGRAM_ID,
          this.poolTokenProgramId,
          destinationTokenProgramId,
          destinationTokenAmount,
          maximumPoolTokenAmount,
        ),
      ),
      [userTransferAuthority],
      confirmOptions,
    );
  }

  static withdrawSingleTokenTypeExactAmountOutInstruction(
    pool: PublicKey,
    curve: PublicKey,
    authority: PublicKey,
    userTransferAuthority: PublicKey,
    poolMint: PublicKey,
    feeAccount: PublicKey,
    sourcePoolAccount: PublicKey,
    fromA: PublicKey,
    fromB: PublicKey,
    userAccount: PublicKey,
    destinationMint: PublicKey,
    swapProgramId: PublicKey,
    poolTokenProgramId: PublicKey,
    destinationTokenProgramId: PublicKey,
    destinationTokenAmount: number | Numberu64,
    maximumPoolTokenAmount: number | Numberu64,
  ): TransactionInstruction {
    return Instructions.withdrawSingleTokenType(
      {
        destinationTokenAmount: new Numberu64(destinationTokenAmount),
        maximumPoolTokenAmount: new Numberu64(maximumPoolTokenAmount),
      },
      {
        signer: userTransferAuthority,
        pool,
        swapCurve: curve,
        poolAuthority: authority,
        destinationTokenMint: destinationMint,
        tokenAVault: fromA,
        tokenBVault: fromB,
        poolTokenMint: poolMint,
        poolTokenFeesVault: feeAccount,
        destinationTokenUserAta: userAccount,
        poolTokenUserAta: sourcePoolAccount,
        poolTokenProgram: poolTokenProgramId,
        destinationTokenProgram: destinationTokenProgramId,
      },
    );
  }
}
