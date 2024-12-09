{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    cargo
    rustfmt
    openssl
    pkg-config
    websocat
  ];
}
