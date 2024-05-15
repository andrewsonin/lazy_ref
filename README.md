# lazy_ref

_Implements a non-blocking synchronization primitive for lazy-initialized immutable references._

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/lazy_ref.svg
[crates-url]: https://crates.io/crates/lazy_ref
[docs-badge]: https://img.shields.io/docsrs/lazy_ref
[docs-url]: https://docs.rs/lazy_ref
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/andrewsonin/lazy_ref/blob/main/LICENSE
[actions-badge]: https://github.com/andrewsonin/lazy_ref/actions/workflows/ci.yml/badge.svg
[actions-url]: https://github.com/andrewsonin/lazy_ref/actions/workflows/ci.yml

## Usage

Writing to a `LazyRef` from separate threads:

```rust
use rayon::prelude::*;
use lazy_ref::LazyRef;

let lazy_ref = LazyRef::new();
let thread_ids: Vec<usize> = vec![1, 2, 3];

thread_ids.par_iter()
    .for_each(
        |id| {
           let r = lazy_ref.get_or_init(|| id);
           assert!(thread_ids.contains(r));
       }
    );
let x = lazy_ref.get().unwrap();
assert!(thread_ids.contains(x));
```