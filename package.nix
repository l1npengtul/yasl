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
      "serenity-0.12.5" = "sha256-SEuJbFx3f7/aMWZsU5lwUDuz5SoFGm9DTVhRVdA0AmU=";
    };
  };

  nativeBuildInputs = [
    pkg-config
    sqlx-cli
  ];
  buildInputs = [ openssl ];

  configurePhase = ''
    export DATABASE_URL="sqlite://yasl.sql"
    sqlx database create
    sqlx database setup
    cargo sqlx prepare
  '';

})
