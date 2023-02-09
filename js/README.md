# Hyperplane JavaScript API

The Hyperplane JavaScript library comprises:

* A library to interact with the on-chain program
* A test client that exercises the program
* Scripts to facilitate building the program

## Getting Started

First fetch the npm dependencies, including `@solana/web3.js`, by running:
```sh
$ yarn install
```

### Build the on-chain program

```sh
$ yarn build:program
```

### Generate anchor bindings from IDL

```sh
$ yarn anchor-client-gen:mainnet
```

### Run the test client

```sh
$ yarn start-with-test-validator
```

// todo - elliot - remove below when not needed
```sh
cargo install --git https://github.com/hubbleprotocol/anchor --branch tk22 anchor-cli --locked --force
```
