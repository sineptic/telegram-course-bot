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
      libPath = with pkgs;
        lib.makeLibraryPath [
          openssl
        ];
    in
      pkgs.mkShell {
        buildInputs = [
          pkgs.openssl
          pkgs.pkg-config
        ];

        RUST_LOG = "debug";
        LD_LIBRARY_PATH = libPath;

        shellHook = ''
          exec fish
        '';
      };
  };
}
