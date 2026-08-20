{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        rustbin = pkgs.rust-bin.selectLatestNightlyWith (
          toolchain:
          toolchain.default.override {
            extensions = [
              "rust-src"
              "clippy"
              "rustfmt"
              "miri"
              "rust-analyzer"
            ];
          }
        );
        yaslpkg = pkgs.callPackage ./package.nix { };
      in
      {
        formatter = pkgs.alejandra;

        packages.default = yaslpkg;

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustbin
            llvmPackages.libclang.lib
            llvmPackages.clang
            lldb
            pkg-config
            cmake
            vcpkg
            rustPlatform.bindgenHook
            rustup
            sqlx-cli
            openssl
          ];

          env.RUST_SRC_PATH = "${rustbin}/lib/rustlib/src/rust/library";
          env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          env.DATABASE_URL = "sqlite://yasl.sql";
          env.YASLTEST = 1;

          shellHook = ''
            export CARGO="$(which cargo)"
            echo "WONDERHOOOOOY!!!!"
          '';
        };
      }
    );
}
