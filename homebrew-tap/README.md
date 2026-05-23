# homebrew-qmd

This is the official Homebrew tap for [QMD-Rust](https://github.com/simonellefsen/qmd-rust).

## Installation

```bash
brew tap simonellefsen/qmd
brew install qmd
```

## Updating

The tap is automatically kept up to date by the release workflow in the main `qmd-rust` repository using `cargo-dist`.

## Development / Manual Update

If you need to update the formula manually:

```bash
# Inside this repo
cp /path/to/new/qmd.rb Formula/qmd.rb
git add Formula/qmd.rb
git commit -m "chore: update qmd to vX.Y.Z"
git push
```

## License

The tap itself is MIT. The QMD-Rust binary is also MIT.
