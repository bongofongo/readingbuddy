# Fuzzing

Two targets, run nightly (`.github/workflows/fuzz.yml`) — never on a PR, since
this needs nightly and a wall-clock budget.

```
cargo +nightly fuzz run -s none parse_sidecar -- -timeout=10 fuzz/seeds/parse_sidecar
cargo +nightly fuzz run -s none epub_info     -- -timeout=10 fuzz/seeds/epub_info
```

## `-s none` is required, and it is measured

AddressSanitizer does not work here. mlua's vendored Lua propagates errors with
`longjmp`, which ASAN's interceptors do not survive on this platform: with ASAN
the target pins a core and makes no forward progress. Without it:

| target | execs | time |
|---|---|---|
| `parse_sidecar` | 741,558 | 26s |
| `epub_info` | 109,615 | 26s |

libFuzzer's coverage feedback, panic detection, and `-timeout` hang detection
all still work. What is lost is memory-error detection inside the C
dependencies (Lua, zlib).

## `-timeout=10` is load-bearing

It is how a non-terminating Lua chunk is detected. `StdLib::NONE` removes the
standard library but not the ability to loop, so `while true do end` in a file
off someone's e-reader is a real denial of service — bounded by
`LUA_INSTRUCTION_BUDGET` in `koreader.rs`, and this flag is what would catch a
regression in that bound.

That budget was originally 50M, which was wrong for a reason worth remembering:
it has to bite quickly under *instrumentation*, not just in a release build. At
50M the fuzzer spent minutes on a single looping input. It is now 5M — still
~50x headroom over the largest plausible real sidecar (the 5000-highlight scale
fixture is ~10^5 instructions).

## Seeds are committed; corpus and artifacts are not

`fuzz/seeds/<target>/` is a small, hand-picked, read-only set drawn from the
test fixtures. **Every crash this ever finds should be minimized and committed
there**, because `crates/engine/tests/fuzz_seeds.rs` replays the whole seed
directory on *stable*, in milliseconds, on every PR. That replay is what makes
fuzzing pay off over time — the nightly job finds things, the seed replay keeps
them found. A fuzzer that has to be re-run to re-catch its own findings catches
them once.
