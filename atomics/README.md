# rustcommon-atomics

Atomic types provided with unifying traits for use in generic programming

## Overview

This crate provides wrappers around the atomics found in the rust core library
with the addition of atomic floating point types. The types exported from this
crate are unified through sets of traits which define operations which may be
performed on the atomic types. This makes it possible to use atomic types with
generic programming.

## Getting Started

### Building

rustcommon is built with the standard Rust toolchain which can be installed and
managed via [rustup](https://rustup.rs) or by following the directions on the
Rust [website](https://www.rust-lang.org/).

#### View library documentation
```bash
cargo doc --open
```

## Support

Create a [new issue](https://github.com/twitter/rustcommon/issues/new) on GitHub.

## Contributing

We feel that a welcoming community is important and we ask that you follow
Twitter's [Open Source Code of Conduct] in all interactions with the community.

## Authors

* Brian Martin <bmartin@twitter.com>

A full list of [contributors] can be found on GitHub.

Follow [@TwitterOSS](https://twitter.com/twitteross) on Twitter for updates.

## License

Copyright 2019-2020 Twitter, Inc.

Licensed under the Apache License, Version 2.0:
https://www.apache.org/licenses/LICENSE-2.0

## Security Issues?

Please report sensitive security issues via Twitter's bug-bounty program
(https://hackerone.com/twitter) rather than GitHub.

[contributors]: https://github.com/twitter/rustcommon/graphs/contributors?type=a
[Open Source Code of Conduct]: https://github.com/twitter/code-of-conduct/blob/master/code-of-conduct.md
