# hawk

`hawk` is an experimental Cargo lint tool for binary products built from
internal Rust workspace crates. It analyzes a selected binary as a closed
world and reports public library items that are not needed by that product or
whose visibility exceeds the needs of the product.

This repository is at the prototype stage.

