# Token-swap JavaScript API

The Token-swap JavaScript library comprises:

* A library to interact with the on-chain program
* A test client that exercises the program
* Scripts to facilitate building the program

## Getting Started

First fetch the npm dependencies, including `@solana/web3.js`, by running:
```sh
$ npm install
```

### Build the on-chain program

```sh
$ npm run build:program
```

### Run the test client

```sh
$ npm run start-with-test-validator
```

todo - remove below when not needed
```sh
cargo install --git https://github.com/hubbleprotocol/anchor --branch token-2022 anchor-cli --locked --force
```
