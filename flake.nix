{
  description = "Restic REST backend for 123pan cloud storage";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = {
    self,
    nixpkgs,
    ...
  }: let
    systems = [
      "aarch64-linux"
      "x86_64-linux"
    ];
    forAllSystems = nixpkgs.lib.genAttrs systems;
  in {
    packages = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      default = pkgs.rustPlatform.buildRustPackage {
        pname = "restic-123pan";
        version = "0.3.1";
        src = ./.;

        cargoHash = "sha256-uVjvjuwknBnvux+Cm3jxS/KD90Eed6bD1q/zQTjgrgU=";
        nativeBuildInputs = [
          pkgs.perl
          pkgs.pkg-config
        ];

        meta = {
          description = "Restic REST backend for 123pan cloud storage";
          homepage = "https://github.com/gaoyifan/restic-123pan";
          license = pkgs.lib.licenses.mit;
          mainProgram = "restic-123pan";
        };
      };
    });

    devShells = forAllSystems (system: {
      default = nixpkgs.legacyPackages.${system}.mkShell {
        packages = with nixpkgs.legacyPackages.${system}; [
          cargo
          rustc
          rustfmt
        ];
      };
    });

    formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.alejandra);

    nixosModules.default = import ./nixos-module.nix {inherit self;};
  };
}
