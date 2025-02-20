{
  inputs = {
    nixpkgs = {
      url = "github:NixOS/nixpkgs/nixos-24.11";
    };
  };
  outputs = {nixpkgs, ...}: let
    system = "x86_64-linux";
  in {
    devShells."${system}".default = let
      pkgs = import nixpkgs {
        inherit system;
      };
    in
      pkgs.mkShell {
        buildInputs = [
          pkgs.pkg-config
        ];

        RUST_LOG = "debug";

        shellHook = ''
          exec fish
        '';
      };
  };
}
