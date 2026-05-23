class Qmd < Formula
  desc "Secure on-device search engine for markdown notes and wikis (Rust port)"
  homepage "https://github.com/simonellefsen/qmd-rust"
  license "MIT"

  # This file is a placeholder.
  # On real releases, cargo-dist will overwrite this with the correct
  # version, URLs, and shasums for prebuilt binaries.

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/simonellefsen/qmd-rust/releases/download/v0.2.0/qmd-v0.2.0-aarch64-apple-darwin.tar.xz"
      sha256 "PLACEHOLDER_SHA256_ARM64"
    else
      url "https://github.com/simonellefsen/qmd-rust/releases/download/v0.2.0/qmd-v0.2.0-x86_64-apple-darwin.tar.xz"
      sha256 "PLACEHOLDER_SHA256_X86_64"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/simonellefsen/qmd-rust/releases/download/v0.2.0/qmd-v0.2.0-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "PLACEHOLDER_SHA256_LINUX_ARM64"
    else
      url "https://github.com/simonellefsen/qmd-rust/releases/download/v0.2.0/qmd-v0.2.0-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "PLACEHOLDER_SHA256_LINUX_X86_64"
    end
  end

  def install
    bin.install "qmd"
  end

  test do
    assert_match "qmd", shell_output("#{bin}/qmd --version")
  end
end
