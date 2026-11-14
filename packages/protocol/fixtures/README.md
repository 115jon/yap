# Protocol fixtures

Golden JSON samples of the Yap wire protocol: gateway frames (`{op, d}`) and
REST bodies. JSON only, no code. Every id, token and credential is fake
(`*_fixture*`, `tok_fixture`) and hosts end in `.invalid`.

Both sides may test against these files:

- Rust: `native/rust/crates/yap-protocol/tests/fixtures.rs` decodes
  each one into its typed struct.
- TypeScript: a vitest can decode the same files with the schemas in `src/`.

When adding one, copy the shape from a real frame and scrub it, keep optional
and unknown fields where the server sends them, and add a decoder test on the
side that uses it. Frame files start with `op`; REST files are the bare
response body.
