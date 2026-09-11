//! Host-side unit tests for tiny_os pure-logic algorithms.
//!
//! These test functions extracted from the kernel that have no hardware
//! dependencies — checksums, header parsing, data structure operations.
//! Run with: cargo test -p host-tests

mod ipv4;
mod ethernet;
mod mbr;
