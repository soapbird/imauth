# Local Cargo patches

## `chromiumoxide_types`

The root workspace manifest contains this crates.io override:

```toml
[patch.crates-io]
chromiumoxide_types = { path = "patches/chromiumoxide_types" }
```

The checked-in patch is version `0.9.1`. It was introduced with the local
chromiumoxide integration in commit `22286ee`. `cargo tree -i
chromiumoxide_types` shows that this path package is used by
`chromiumoxide`, `chromiumoxide_cdp`, and `chromiumoxide_pdl`. Keep the
override and its vendored package aligned: removing it silently changes the
dependency source for all three crates.

The repository history records the vendor import and override, but does not
record a narrower behavioral rationale. Do not infer one when updating it;
compare the actual upstream source and preserve every intentional local delta.

## Safe update protocol

1. Work in a clean, dedicated change and identify the target upstream version
   in `Cargo.toml` and `Cargo.lock`.
2. Download the candidate crate into a temporary directory, then compare it
   against this patch before editing. For the current version:

   ```bash
   patch_tmp_dir=$(mktemp -d)
   curl --fail --location --max-time 60 \
     https://crates.io/api/v1/crates/chromiumoxide_types/0.9.1/download \
     --output "$patch_tmp_dir/chromiumoxide_types-0.9.1.crate"
   tar -xzf "$patch_tmp_dir/chromiumoxide_types-0.9.1.crate" -C "$patch_tmp_dir"
   diff -ru "$patch_tmp_dir/chromiumoxide_types-0.9.1" patches/chromiumoxide_types
   ```

3. Apply the smallest intentional patch delta, retain upstream license files,
   and update the root override only if its version/source must change. Do not
   edit unrelated vendored crates.
4. Run the exact gates before committing:

   ```bash
   cargo fmt --all -- --check
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo test --manifest-path patches/chromiumoxide_types/Cargo.toml
   cargo tree -i chromiumoxide_types
   ```

5. Review `git diff --check`, remove the temporary download directory, and
   commit the patch update atomically. Do not push or publish without explicit
   authorization.
