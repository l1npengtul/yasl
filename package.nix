{
  lib,
  rustPlatform,
  pkg-config,
  sqlx-cli,
  openssl,
}:
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "yasl";
  version = "0.1.0";

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes = {
      "poise-0.6.1" = "sha256-LYGoUUXFy3gEpnQtWxNsBbcKdeBallENTq1mfvVkTn4=";
    };
  };

  nativeBuildInputs = [
    pkg-config
    sqlx-cli
  ];
  buildInputs = [ openssl ];
})
