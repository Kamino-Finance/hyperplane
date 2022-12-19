---
title: Token-2022 Program
---

A token program on the Solana blockchain, defining a common implementation for
fungible and non-fungible tokens.

The Token-2022 Program is a superset of the functionality provided by the
[Token Program](token.mdx), deployed to all networks.

| Information | Account Address |
| --- | --- |
| Token-2022 Program | `TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb` |

## Motivation

The existing Token Program serves most needs for fungible and non-fungible tokens
on Solana through a simple set of interfaces and structures. It has been rigorously
audited since its initial deployment in 2020.

As more developers have come to Solana with new ideas, however, they have forked the
Token Program to add functionality. It's simple to change and deploy the program,
but it's difficult to achieve adoption across the ecosystem.

Solana's programming model requires programs to be included in transactions
along with accounts, making it complicated to craft transactions involving
multiple token programs.

On top of the technical difficulty, wallets and on-chain programs must trust any
token program that they choose to support.

We need to add new token functionality, with minimal disruption to users, wallets,
and dApps. Most importantly, we must preserve the safety of existing tokens.

A new token program, Token-2022, was developed to achieve both of these goals,
deployed to a different address than the Token program.

## Concept

To make adoption as easy as possible, the functionality and structures in
Token-2022 are a strict superset of Token.

### Instructions

Token-2022 supports the exact same instruction layouts as Token, byte for
byte. For example, if you want to transfer 100 tokens on a mint with 2 decimals,
you create a `TransferChecked` instruction, with this byte-represented data:

```
[12, 100, 0, 0, 0, 0, 0, 0, 0, 2]
 ^^ TransferChecked enum
     ^^^^^^^^^^^^^^^^^^^^^^^^ 100, as a little-endian 64-bit unsigned integer
                               ^ 2, as a byte
```

This format means the exact same thing to both Token and Token-2022. If you want
to target one program over another, you just need to change the `program_id` in
the instruction.

All new instructions in Token-2022 start where Token stops. Token has 25 unique
instructions, with indices 0 through 24. Token-2022 supports all of these
instructions, and then adds new functionality at index 25.

There are no plans to ever add new instructions to Token.

### Mints and Accounts

For structure layouts, the same idea mostly applies. An `Account` has the same
exact representation between Token and Token-2022 for the first `165` bytes, and
a `Mint` has the same representation for the first `82` bytes.

### Extensions

New functionality requires new fields in mints and accounts, which
makes it impossible to have the exact same layout for all accounts in Token-2022.

New fields are added in the form of extensions.

Mint creators and account owners can opt-in to Token-2022 features. Extension data
is written after the end of the `Account` in Token, which is the byte at index
`165`.  This means it is always possible to differentiate mints and accounts.

You can read more about how this is done at the
[source code](https://github.com/solana-labs/solana-program-library/blob/master/token/program-2022/src/extension/mod.rs).

Mint extensions currently include:

* confidential transfers
* transfer fees
* closing mint
* interest-bearing tokens
* non-transferable tokens

Account extensions currently include:

* memo required on incoming transfers
* immutable ownership
* default account state

Extensions can be mixed and matched, which means it's possible to create a mint
with only transfer fees, only interest-bearing tokens, both, or neither!

### Associated Token Accounts

To make things simpler, there is still only one associated token account
program, that creates new token accounts for either Token or Token-2022.

## Getting Started

To get started with Token-2022:

- [Install the Solana Tools](https://docs.solana.com/cli/install-solana-cli-tools)
- [Extension Guide](token-2022/extensions.mdx)
- [Wallet Guide](token-2022/wallet.md)
- [On-Chain Program Guide](token-2022/onchain.md)

For existing functionality in the Token Program, see the [token docs](token.mdx).
The Token functionality will always apply to Token-2022.

## Source

The Token-2022 Program's source is available on
[GitHub](https://github.com/solana-labs/solana-program-library/tree/master/token/program-2022).

For information about the types and instructions, the Rust docs are available at
[docs.rs](https://docs.rs/spl-token-2022/latest/spl_token_2022/).

## Security Audits

The Token-2022 Program is currently under multiple audits to ensure safety of
funds. All audits will be published here as they are completed.

Here are the completed audits as of 12 December 2022:

* Halborn
    - Review commit hash [`c3137a`](https://github.com/solana-labs/solana-program-library/tree/c3137af9dfa2cc0873cc84c4418dea88ac542965/token/program-2022)
    - Final report https://github.com/solana-labs/security-audits/blob/master/spl/HalbornToken2022Audit.pdf
* Zellic
    - Review commit hash [`54695b`](https://github.com/solana-labs/solana-program-library/tree/54695b233484722458b18c0e26ebb8334f98422c/token/program-2022)
    - Final report https://github.com/solana-labs/security-audits/blob/master/spl/ZellicToken2022Audit.pdf

Here are the ongoing audits:

* NCC Group
* Trail of Bits

## Status and Upgradability

The Token-2022 program is still under audit and not meant for full production use.
All clusters have the latest program deployed **for testing and development purposes
ONLY**.

To facilitate deploying updates and security fixes, the program deployment remains
upgradable. Once audits are complete, the deployment will be marked final and no
further upgrades will be possible. This is expected to happen sometime in Q1 2023.
