{pkgs ? import <nixpkgs> {}}: let
  libPath = with pkgs;
    lib.makeLibraryPath [
      openssl
    ];
in {
  devShell = with pkgs;
    mkShell {
      buildInputs = [
        pkg-config
        openssl
      ];

      RUST_LOG = "debug";
      LD_LIBRARY_PATH = libPath;
    };
}
