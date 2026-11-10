# Environnement de dev NixOS :
#   nix-shell --run 'cargo run -p cxssm-app --release'
{ pkgs ? import <nixpkgs> { } }:
pkgs.mkShell rec {
  nativeBuildInputs = with pkgs; [
    # Toolchain Rust (nixpkgs ; le projet demande Rust >= 1.85)
    cargo rustc clippy rustfmt rust-analyzer
    pkg-config cmake clang
  ];
  buildInputs = with pkgs; [
    openssl fontconfig freetype libxkbcommon wayland vulkan-loader
    libx11 libxcb libxcursor libxrandr libxi
    alsa-lib zstd
    # Requis par l'outil FMOD `bin/fsbank` (binaire précompilé pour distros classiques)
    stdenv.cc.cc.lib glib zlib
  ];
  # Bibliothèques chargées dynamiquement par GPUI (Vulkan, Wayland, X11…)
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
  # Sources de la std pour rust-analyzer
  RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
}
