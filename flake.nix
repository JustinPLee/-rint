{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    nixpkgs,
    utils,
    ...
  }:
    utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          rustup
          just
          gcc
          cargo-watch
          cargo-outdated
          cargo-edit
          cargo-audit
          cargo-insta
        ];

        shellHook = ''
          export RUST_BACKTRACE=1
          echo "Env loaded."
        '';
      };
    });
}
