I was about to read [this blog post](https://barrcodes.dev/posts/1brc/) about the [The One Billion Row Challenge](https://github.com/gunnarmorling/1brc), but decided to attempt it myself before spoiling it.

For my current attempt in the repo, I set myself the following rules:

1. No reading anyone else's attempts.
2. Safe Rust only.
3. No assuming anything about the input data, except what is stated in the challenge brief: each line of the input is of the form `<string: station name>;<double: measurement>`. Notably this means we can't look into the code that generates the input and make assumptions about the size and number of the station names, which makes it harder to use techniques like SIMD, lookup tables, perfect hash functions etc.

I intend on making separate attempts that don't have these rules.

**I've only committed changes that improve the performance, and each commit contains the time and changes made.**

## Running

First, install the Rust nightly tool chain specified in `./rust-toolchain`.

Then generate the input data (which I've assumed is in `./measurements.txt`), as well as the reference output data (which I've assumed is in `./reference.txt`), the instructions for which are in the two two links above.

Before running you'll want to load the input data into the page cache, since we're interested in the speed of our code, not our disk. You can do this using [vmtouch](https://github.com/hoytech/vmtouch) (kill the process to release the memory):

```sh
vmtouch -l ./measurements.txt
```

Then, in a separate terminal window, compile the program in release mode:

```sh
cargo build --release
```

Finally you can benchmark the code using [hyperfine](https://github.com/sharkdp/hyperfine):

```sh
hyperfine -r 3 ./target/release/challenge
```

and compare our output to the output of the reference implementation:

```sh
./target/release/challenge > out.txt
```

```sh
cmp out.txt reference.txt
```

If `cmp` returns no output then the two files are identical 🎉.

## Other commands

To profile using [samply](https://github.com/mstange/samply):

```sh
cargo build --profile profiling
```

```sh
samply record ./target/profiling/challenge > out.txt
```

To profile with instruments on MacOS (using [cargo-instruments](https://github.com/cmyr/cargo-instruments)):

```sh
cargo instruments -t time --profile profiling > out.txt
```
