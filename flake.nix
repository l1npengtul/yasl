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
    )
    // {
      nixosModules.default =
        {
          config,
          pkgs,
          lib,
          ...
        }:
        let
          cfg = config.services.yasl;
          format = pkgs.formats.toml { };
        in
        {
          options.services.yasl = {
            enable = lib.mkEnableOption "enable YASL discord bot";

            settings = lib.mkOption {
              description = ''
                Structured configurations of atticd.
              '';
              type = format.type;
              default = { };
            };

            configFile = lib.mkOption {
              description = ''
                Path to an existing atticd configuration file.

                By default, it's generated from `services.atticd.settings`.
              '';
              type = lib.types.path;
              default = format.generate "yasl.toml" cfg.settings;
              defaultText = "generated from `services.atticd.settings`";
            };
          };

          config = lib.mkIf cfg.enable {
            environment.systemPackages = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];

            users.users.yasl = {
              description = "yasl bot service user";
              isSystemUser = true;
              group = "yasl";
            };
            users.groups.yasl = { };
            systemd.services.yasl =
              let
                sys = pkgs.stdenv.hostPlatform.system;
                yaslpkg = self.packages.${sys}.default;
              in
              {
                wantedBy = [ "default.target" ];
                after = [ "network.target" ];
                description = "yasl discord bot";
                serviceConfig = {
                  User = "yasl";
                  Group = "yasl";
                  StateDirectory = "yasl";
                  Restart = "on-failure";
                  RestartSec = 10;
                  ExecStart = "${yaslpkg}/bin/yasl";

                  CapabilityBoundingSet = [ "" ];
                  DeviceAllow = "";
                  DevicePolicy = "closed";
                  LockPersonality = true;
                  MemoryDenyWriteExecute = true;
                  NoNewPrivileges = true;
                  PrivateDevices = true;
                  PrivateTmp = true;
                  PrivateUsers = true;
                  ProcSubset = "pid";
                  ProtectClock = true;
                  ProtectControlGroups = true;
                  ProtectHome = true;
                  ProtectHostname = true;
                  ProtectKernelLogs = true;
                  ProtectKernelModules = true;
                  ProtectKernelTunables = true;
                  ProtectProc = "invisible";
                  ProtectSystem = "strict";
                  ReadWritePaths = [ "/var/lib/yasl" ];
                  RemoveIPC = true;
                  RestrictAddressFamilies = [
                    "AF_INET"
                    "AF_INET6"
                    "AF_UNIX"
                  ];
                  RestrictNamespaces = true;
                  RestrictRealtime = true;
                  RestrictSUIDSGID = true;
                  SystemCallArchitectures = "native";
                  SystemCallFilter = [
                    "@system-service"
                    "~@resources"
                    "~@privileged"
                  ];
                  UMask = "0077";
                };

                environment = {
                  RUST_LOG = "yasl";
                };
              };

            environment.etc."yasl.toml".source = cfg.configFile;
          };
        };
    };
}
