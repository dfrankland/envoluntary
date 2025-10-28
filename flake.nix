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
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [
        # To import a flake module
        # 1. Add foo to inputs
        # 2. Add foo as a parameter to the outputs function
        # 3. Add here: foo.flakeModule
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
            p.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default)
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
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            inputs.rust-overlay.overlays.default
            inputs.devshell.overlays.default
          ];
        };

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
            }
          );
        };

        packages = {
          default = envoluntary;
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
          devshellDevShell = craneLib.devShell.override {
            mkShell = {
              inputsFrom,
              packages,
            }:
              pkgs.devshell.mkShell {
                imports = [(pkgs.devshell.importTOML ./devshell.toml)];
                packagesFrom = inputsFrom;
                inherit packages;
              };
          };
        in
          devshellDevShell {
            checks = inputs.self.checks.${system};
            packages = [];
          };
      };
      flake = {
        # The usual flake attributes can be defined here, including system-
        # agnostic ones like nixosModule and system-enumerating ones, although
        # those are more easily expressed in perSystem.
      };
    };
}
