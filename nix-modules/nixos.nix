{
  lib,
  config,
  pkgs,
  ...
}: let
  cfg = config.programs.envoluntary;
  enabledOption = x:
    lib.mkEnableOption x
    // {
      default = true;
      example = false;
    };
  format = pkgs.formats.toml {};
in {
  options.programs.envoluntary = {
    enable = lib.mkEnableOption ''
      envoluntary integration. Takes care of both installation and setting up
      the sourcing of the shell. Note that you need to logout and login for this
      change to apply.
    '';

    package = lib.mkPackageOption pkgs "envoluntary" {};

    finalPackage = lib.mkOption {
      type = lib.types.package;
      readOnly = true;
      description = "The wrapped envoluntary package.";
    };

    enableBashIntegration = enabledOption ''
      Bash integration
    '';
    enableZshIntegration = enabledOption ''
      Zsh integration
    '';
    enableFishIntegration = enabledOption ''
      Fish integration
    '';

    loadInNixShell = enabledOption ''
      loading envoluntary in `nix-shell` `nix shell` or `nix develop`
    '';

    config = lib.mkOption {
      inherit (format) type;
      default = {};
      example = lib.literalExpression ''
        {
          entries = [
            {
              pattern = ".*";
              pattern_adjacent = ".*/Cargo\\.toml";
              flake_reference = "github:NixOS/templates/30a6f18?dir=rust";
            }
          ];
        }
      '';
      description = ''
        Envoluntary configuration. Refer to https://github.com/dfrankland/envoluntary/blob/main/README.md`.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    programs = {
      envoluntary = {
        finalPackage = pkgs.symlinkJoin {
          inherit (cfg.package) name;
          paths = [cfg.package];
          meta.mainProgram = "envoluntary";
        };
      };

      zsh.interactiveShellInit = lib.mkIf cfg.enableZshIntegration ''
        if ${lib.boolToString cfg.loadInNixShell} || printenv PATH | grep -vqc '/nix/store'; then
          eval "$(${lib.getExe cfg.finalPackage} shell hook zsh)"
        fi
      '';

      #$NIX_GCROOT for "nix develop" https://github.com/NixOS/nix/blob/6db66ebfc55769edd0c6bc70fcbd76246d4d26e0/src/nix/develop.cc#L530
      #$IN_NIX_SHELL for "nix-shell"
      bash.interactiveShellInit = lib.mkIf cfg.enableBashIntegration ''
        if ${lib.boolToString cfg.loadInNixShell} || [ -z "$IN_NIX_SHELL$NIX_GCROOT$(printenv PATH | grep '/nix/store')" ] ; then
          eval "$(${lib.getExe cfg.finalPackage} shell hook bash)"
        fi
      '';

      fish.interactiveShellInit = lib.mkIf cfg.enableFishIntegration ''
        if ${lib.boolToString cfg.loadInNixShell};
        or printenv PATH | grep -vqc '/nix/store';
          ${lib.getExe cfg.finalPackage} shell hook fish | source
        end
      '';
    };

    environment = {
      systemPackages = [
        cfg.finalPackage
      ];

      variables.ENVOLUNTARY_CONFIG_PATH = "/etc/envoluntary/config.toml";

      etc = {
        "envoluntary/config.toml" = lib.mkIf (cfg.config != {}) {
          source = format.generate "config.toml" cfg.config;
        };
      };
    };
  };
}
