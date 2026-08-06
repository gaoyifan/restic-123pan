{self}: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.restic-123pan;
in {
  options.services.restic-123pan = {
    enable = lib.mkEnableOption "restic-123pan backend";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      description = "restic-123pan package to run.";
    };

    environmentFile = lib.mkOption {
      type = lib.types.externalPath;
      description = "Environment file containing the 123pan credentials.";
    };

    cacheDirectory = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/restic-123pan";
      description = "Directory containing the restic-123pan SQLite cache.";
    };

    repositoryPath = lib.mkOption {
      type = lib.types.str;
      default = "/restic-backup";
      description = "Root folder path on 123pan for the repository.";
    };

    listenAddress = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = "Address on which restic-123pan listens.";
    };

    listenPort = lib.mkOption {
      type = lib.types.port;
      default = 8000;
      description = "Port on which restic-123pan listens.";
    };

    forceCacheRebuild = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Force rebuilding the restic-123pan cache on startup.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "root";
      description = "User running restic-123pan.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "root";
      description = "Group running restic-123pan.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.tmpfiles.rules = [
      "d ${cfg.cacheDirectory} 0750 ${cfg.user} ${cfg.group} -"
    ];

    systemd.services.restic-123pan = {
      description = "Restic REST backend for 123pan cloud storage";
      wantedBy = lib.mkDefault ["multi-user.target"];
      after = ["network-online.target"];
      wants = ["network-online.target"];
      unitConfig.RequiresMountsFor = [cfg.cacheDirectory];
      serviceConfig = {
        ExecStart = lib.getExe cfg.package;
        EnvironmentFile = cfg.environmentFile;
        User = cfg.user;
        Group = cfg.group;
        Restart = "on-failure";
        RestartSec = "5s";
      };
      environment = {
        LISTEN_ADDR = cfg.listenAddress;
        LISTEN_PORT = toString cfg.listenPort;
        PAN123_REPO_PATH = cfg.repositoryPath;
        FORCE_CACHE_REBUILD = lib.boolToString cfg.forceCacheRebuild;
        DB_PATH = "${cfg.cacheDirectory}/cache-123pan.db";
      };
    };
  };
}
