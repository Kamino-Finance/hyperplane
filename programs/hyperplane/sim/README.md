## Hyperspace simulation

An off-chain implementation of the stable swap invariant, written in Rust

Differences from smart contract impl:

- Use unlimited size numbers (BigInt), scaled to 18 dp
- Unlimited iterations to converge on y or D
- Use negative numbers when solving y
- Uses standard (unchecked) arithmetic - the simulation is expected to run under test or debug mode therefore overflow checks will be enabled
