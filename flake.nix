{
  description = "Envoluntary: Automatic Nix development environments for your shell.";

  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    devshell.url = "github:numtide/devshell";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [
        inputs.flake-parts.flakeModules.easyOverlay
        inputs.home-manager.flakeModules.home-manager
      ];
      systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin"];
      perSystem = {
        config,
        self',
        inputs',
        pkgs,
        system,
        ...
      }: let
        craneLib = (inputs.crane.mkLib pkgs).overrideToolchain (
          p:
          # NB: use nightly for https://github.com/rust-lang/rustfmt/issues/6241
            (inputs.rust-overlay.lib.mkRustBin {} p).selectLatestNightlyWith (toolchain:
              toolchain.default.override {
                extensions = ["rust-src"] ++ pkgs.lib.optionals pkgs.stdenv.isLinux ["llvm-tools-preview"];
              })
        );
        src = craneLib.cleanCargoSource ./.;
        commonArgs = {
          inherit src;
          strictDeps = true;
        };
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        individualCrateArgs =
          commonArgs
          // {
            inherit cargoArtifacts;
            inherit (craneLib.crateNameFromCargoToml {inherit src;}) version;
            # NB: run tests via cargo-nextest
            doCheck = false;
          };
        fileSetForCrate = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            (craneLib.fileset.commonCargoSources ./cli)
            (craneLib.fileset.commonCargoSources ./env-hooks)
            (craneLib.fileset.commonCargoSources ./nix-dev-env)
          ];
        };
        envoluntary = craneLib.buildPackage (
          individualCrateArgs
          // {
            pname = "envoluntary";
            meta.mainProgram = "envoluntary";
            cargoExtraArgs = "-p envoluntary";
            src = fileSetForCrate;
          }
        );
        envHooksExampleDirenv = craneLib.buildPackage (
          individualCrateArgs
          // {
            pname = "env-hooks-example-direnv";
            cargoExtraArgs = "-p env-hooks --example direnv";
            src = fileSetForCrate;
          }
        );
      in {
        checks = {
          inherit envoluntary envHooksExampleDirenv;

          clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );

          doc = craneLib.cargoDoc (
            commonArgs
            // {
              inherit cargoArtifacts;
              # This can be commented out or tweaked as necessary, e.g. set to
              # `--deny rustdoc::broken-intra-doc-links` to only enforce that lint
              env.RUSTDOCFLAGS = "--deny warnings";
            }
          );

          fmt = craneLib.cargoFmt {
            inherit src;
          };

          toml-fmt = craneLib.taploFmt {
            src = pkgs.lib.sources.sourceFilesBySuffices src [".toml"];
          };

          nextest = craneLib.cargoNextest (
            commonArgs
            // {
              inherit cargoArtifacts;
              partitions = 1;
              partitionType = "count";
              cargoNextestPartitionsExtraArgs = "--no-tests=pass";
              # Currently only supported on Linux
              # https://github.com/NixOS/nixpkgs/blob/6a08e6bb4e46ff7fcbb53d409b253f6bad8a28ce/pkgs/by-name/ca/cargo-llvm-cov/package.nix#L94-L95
              withLlvmCov = pkgs.stdenv.isLinux;
              nativeBuildInputs = [pkgs.bash pkgs.uutils-coreutils-noprefix];
              # Some tests use scripts which require an absolute path to bash.
              NIX_BIN_BASH = "${pkgs.bash}/bin/bash";
            }
          );
        };

        overlayAttrs = {
          inherit envoluntary;
        };

        packages = {
          default = envoluntary;
          inherit envoluntary;
          inherit envHooksExampleDirenv;
        };

        apps = {
          default = {
            type = "app";
            meta.description = "Automatic Nix development environments for your shell";
            program = pkgs.writeShellScriptBin "envoluntary" ''
              ${envoluntary}/bin/envoluntary "$@"
            '';
          };
          envHooksExampleDirenv = {
            type = "app";
            meta.description = "Example of using env-hooks to implement a direnv-like utility";
            program = pkgs.writeShellScriptBin "direnv" ''
              ${envHooksExampleDirenv}/bin/direnv "$@"
            '';
          };
        };

        devShells.default = let
          devshell = import "${inputs.devshell}/default.nix" {nixpkgs = pkgs;};
          devshellDevShell = craneLib.devShell.override {
            mkShell = {
              inputsFrom,
              packages,
            }:
              devshell.mkShell {
                imports = [(devshell.importTOML ./devshell.toml)];
                packagesFrom = inputsFrom;
                inherit packages;

                env = [
                  {
                    name = "NIX_BIN_BASH";
                    eval = "${pkgs.bash}/bin/bash";
                  }
                ];
              };
          };
        in
          devshellDevShell {
            checks = inputs.self.checks.${system};
            packages = [];
          };
      };
      flake = {
        nixosModules.default = ./nix-modules/nixos.nix;
        homeModules.default = ./nix-modules/home-manager.nix;
      };
    };
}
