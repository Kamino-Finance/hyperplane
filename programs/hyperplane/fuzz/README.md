### Run

Requires `libunwind-dev` + `binutils-dev (libelf/libbfd)` on Linux.

```sh
cargo install honggfuzz
```

```sh
RUST_BACKTRACE=full HFUZZ_RUN_ARGS="--run_time 30 --exit_upon_crash --keep_output" cargo hfuzz run hyperplane-instructions
```
