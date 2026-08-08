{self}: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.restic-123pan;
  instanceModule = {
    options = {
      usernameFile = lib.mkOption {
        type = lib.types.str;
        description = "File containing the 123pan username.";
      };

      passwordFile = lib.mkOption {
        type = lib.types.str;
        description = "File containing the 123pan password.";
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
  };
in {
  options.services.restic-123pan = {
    enable = lib.mkEnableOption "restic-123pan backend";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      description = "restic-123pan package to run.";
    };

    instances = lib.mkOption {
      type = lib.types.attrsOf (lib.types.submodule instanceModule);
      default = {};
      description = "Named restic-123pan backend instances.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.tmpfiles.rules =
      lib.mapAttrsToList (
        _: instance: "d ${instance.cacheDirectory} 0750 ${instance.user} ${instance.group} -"
      )
      cfg.instances;

    systemd.services =
      lib.mapAttrs' (
        name: instance:
          lib.nameValuePair "restic-123pan-${name}" {
            description = "Restic REST backend for 123pan cloud storage (${name})";
            wantedBy = lib.mkDefault ["multi-user.target"];
            after = ["network-online.target"];
            wants = ["network-online.target"];
            unitConfig.RequiresMountsFor = [
              instance.cacheDirectory
              instance.usernameFile
              instance.passwordFile
            ];
            serviceConfig = {
              ExecStart = "${lib.getExe cfg.package} --username-file %d/username --password-file %d/password";
              LoadCredential = [
                "username:${instance.usernameFile}"
                "password:${instance.passwordFile}"
              ];
              User = instance.user;
              Group = instance.group;
              Restart = "on-failure";
              RestartSec = "5s";
            };
            environment = {
              LISTEN_ADDR = instance.listenAddress;
              LISTEN_PORT = toString instance.listenPort;
              PAN123_REPO_PATH = instance.repositoryPath;
              FORCE_CACHE_REBUILD = lib.boolToString instance.forceCacheRebuild;
              DB_PATH = "${instance.cacheDirectory}/cache-123pan.db";
            };
          }
      )
      cfg.instances;
  };
}
