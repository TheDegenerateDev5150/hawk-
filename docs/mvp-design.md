# MVP design

## Product model

The analyzed unit is one explicitly selected Cargo binary target. All
workspace library dependencies compiled into that binary are considered
closed-world unless their crates are explicitly excluded.

The MVP analyzes:

- one binary target;
- `--all-features`;
- the host target;
- production reachability only.

Alternate products, platform configurations, and test-only reachability are
future analysis modes rather than implicit roots.

## Diagnostics

The selected binary seeds reachability but does not itself receive `hawk`
diagnostics. Compiled workspace library dependencies can produce:

- dead public surface: a public declaration is not reachable from the product;
- unnecessary public visibility: a live public declaration has no live
  cross-crate consumer and can be restricted to `pub(crate)`.

The MVP is diagnostic-only. It retains declaration spans and reasoning needed
for follow-up machine-applicable fixes.

## Initial scope

The MVP includes free functions, inherent methods and associated functions,
named types, constants, and statics. Public re-exports are recorded in graph
fragments but do not produce diagnostics yet: rustc resolves downstream paths
to the underlying declaration, so an export-path diagnostic cannot yet prove
which `use` was consumed. Targets of public re-exports are treated as
required-public roots because narrowing only the declaration fails with
`E0365`. The MVP suggests no visibility narrower than `pub(crate)`.

Trait-associated item diagnostics, fields, enum variants, and public module
visibility are deferred. Types assigned by associated type definitions in
publicly reachable trait implementations are treated as required-public roots
because restricting them can make the crate fail to compile (`E0446`) even
without a product call path. Trait implementation bodies are conservatively
rooted so indirect trait dispatch does not turn into dead-public false
positives. Live
cross-crate APIs do not receive diagnostics because `pub` is the narrowest
Rust visibility available for those uses.

Existing `dead_code` lint allowances are treated as deliberate retention. They
do not turn dependencies into production uses: a retained public function may
still reveal that a public helper it uses only internally does not need to be
public.

## Implementation direction

`cargo hawk` invokes the selected Cargo build with
`RUSTC_WORKSPACE_WRAPPER=hawk-driver`. The compiler driver is pinned to the
workspace Rust toolchain and emits resolved graph fragments for each compiled
workspace crate. The frontend merges those fragments and traverses from the
selected binary entry point.

Reference edges distinguish implementation-body reachability from public
interface exposure. If a cross-crate product entry exposes another workspace
type in its public signature, that type is retained as requiring public
visibility.
