# homebrew-qmd

This is the official Homebrew tap for [QMD-Rust](https://github.com/simonellefsen/qmd-rust).

## Installation

```bash
brew tap simonellefsen/qmd
brew install qmd
```

## Updating

The tap is automatically kept up to date by the release workflow in the main `qmd-rust` repository using `cargo-dist`.

## Creating the Tap Repository (first time / bootstrap)

The `homebrew-tap/` directory in the main [qmd-rust](https://github.com/simonellefsen/qmd-rust) repo contains the initial files for this tap.

To create the tap on GitHub for the first time:

1. On GitHub, create a **new public repository** called exactly `homebrew-qmd` under the `simonellefsen` account (full name must be `simonellefsen/homebrew-qmd`).
   - You can initialize it with a default README or leave it empty.
   - Nothing else is required at creation time.

2. (Optional but recommended for testing) Push a prerelease tag (e.g. `v0.2.0-test`) to the main `qmd-rust` repo.
   - The Release workflow will:
     - Build binaries for all supported platforms
     - Generate the real `Formula/qmd.rb` with correct download URLs + checksums
     - Use the `HOMEBREW_TAP_TOKEN` secret (if configured) to push the formula into this tap

3. After the first successful release, users can run:
   ```bash
   brew tap simonellefsen/qmd
   brew install qmd
   ```

If you ever need to manually seed the directory structure:

```bash
git clone https://github.com/simonellefsen/homebrew-qmd.git
cd homebrew-qmd
mkdir -p Formula
# (copy skeleton from qmd-rust/homebrew-tap/Formula/qmd.rb if desired)
git add Formula/
git commit -m "chore: seed Formula directory"
git push
```

## Development / Manual Update

If you need to update the formula manually (rare):

```bash
# After a manual `cargo dist generate-homebrew-formula --tag=vX.Y.Z`
cp target/distrib/qmd.rb /path/to/homebrew-qmd/Formula/qmd.rb
cd /path/to/homebrew-qmd
git add Formula/qmd.rb
git commit -m "chore: update qmd to vX.Y.Z"
git push
```

## License

The tap itself is MIT. The QMD-Rust binary is also MIT.
