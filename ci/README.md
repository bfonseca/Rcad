# CI

`ci.yml` is a GitHub Actions workflow that builds and tests the whole
workspace. It lives here instead of in `.github/workflows/` on purpose:
pushing a file to `.github/workflows/` requires a personal access token with
the `workflow` scope, and the current token does not have it.

To enable it:

```bash
mkdir -p .github/workflows
git mv ci/ci.yml .github/workflows/ci.yml
git commit -m "Enable CI"
git push                      # needs a token with the workflow scope
```

What it does:

- `cargo build --workspace --all-targets` (blocking)
- `cargo test --workspace` (blocking)
- `cargo clippy --workspace --all-targets` (advisory)
- `cargo fmt --all --check` (advisory)
- checks that the icon sprites are present and contain symbols

Both blocking steps are the ones that would have caught the breakage a fresh
clone used to hit: the missing generated sprites, and `rcad-mcp` not compiling.