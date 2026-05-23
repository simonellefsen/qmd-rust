{
  description = "QMD-Rust - Secure on-device search engine for markdown notes (Rust port)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        # Build the Rust binary
        qmd = pkgs.rustPlatform.buildRustPackage {
          pname = "qmd";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.sqlite ];

          # If you decide to use sqlite-vec as a loadable extension,
          # you would add it here and set the appropriate env var / runtime path.
          # For now we keep it simple (bundled rusqlite is used in the code).

          meta = with pkgs.lib; {
            description = "Secure, high-performance Rust implementation of QMD";
            homepage = "https://github.com/simonellefsen/qmd-rust";
            license = licenses.mit;
            mainProgram = "qmd";
          };
        };
      in
      {
        packages = {
          default = qmd;
          qmd = qmd;
        };

        apps.default = {
          type = "app";
          program = "${qmd}/bin/qmd";
        };

        # Future: expose a Home Manager module for declarative `programs.qmd.enable` + index config.
        # See wiki/runbooks/release.md for the planned improvement.

        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.pkg-config
            pkgs.sqlite
            # Add cargo tools you like:
            # pkgs.cargo-watch
            # pkgs.cargo-expand
          ];

          shellHook = ''
            echo "QMD-Rust development shell"
            echo "Run: cargo run -- <command>"
            echo "     cargo fmt && cargo clippy -- -D warnings"
          '';
        };
      }
    );
}