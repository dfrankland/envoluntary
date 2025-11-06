{
  config,
  lib,
  pkgs,
  ...
}: let
  inherit
    (lib)
    mkOption
    mkEnableOption
    mkPackageOption
    mkIf
    mkAfter
    getExe
    ;

  cfg = config.programs.envoluntary;

  tomlFormat = pkgs.formats.toml {};
in {
  options.programs.envoluntary = {
    enable = mkEnableOption "envoluntary, automatic Nix development environments for your shell";

    package = mkPackageOption pkgs "envoluntary" {};

    config = mkOption {
      inherit (tomlFormat) type;
      default = {};
      description = ''
        Configuration written to
        {file}`$XDG_CONFIG_HOME/envoluntary/config.toml`.

        See https://github.com/dfrankland/envoluntary/blob/main/README.md for the full list of options.
      '';
    };

    enableBashIntegration = lib.hm.shell.mkBashIntegrationOption {inherit config;};
    enableZshIntegration = lib.hm.shell.mkZshIntegrationOption {inherit config;};
    enableFishIntegration = lib.hm.shell.mkFishIntegrationOption {inherit config;};
  };

  config = mkIf cfg.enable {
    home.packages = [cfg.package];

    programs = {
      bash.initExtra = mkIf cfg.enableBashIntegration (
        # Using `mkAfter` to make it more likely to appear after other
        # manipulations of the prompt.
        mkAfter ''
          eval "$(${getExe cfg.package} shell hook bash)"
        ''
      );

      fish.interactiveShellInit = mkIf cfg.enableFishIntegration (
        # Using `mkAfter` to make it more likely to appear after other
        # manipulations of the prompt.
        mkAfter ''
          ${getExe cfg.package} shell hook fish | source
        ''
      );

      zsh.initContent = mkIf cfg.enableZshIntegration ''
        eval "$(${getExe cfg.package} shell hook zsh)"
      '';
    };

    xdg.configFile = {
      "envoluntary/config.toml" = mkIf (cfg.config != {}) {
        source = tomlFormat.generate "config.toml" cfg.config;
      };
    };
  };
}
