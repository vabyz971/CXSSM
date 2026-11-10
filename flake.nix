{
  description = "CXSSM — CarX Street Sound Mod : éditeur des sons FMOD de CarX Street";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

      # Bibliothèques chargées à l'exécution :
      # - GPUI : Vulkan, Wayland / X11, polices ;
      # - audio : ALSA (pré-écoute), PulseAudio (sortie de FMOD) ;
      # - outil FMOD `fsbank` (binaire précompilé) : libstdc++, glib, zlib.
      runtimeLibs = pkgs: with pkgs; [
        vulkan-loader wayland libxkbcommon fontconfig freetype
        libx11 libxcb libxcursor libxrandr libxi
        alsa-lib libpulseaudio
        stdenv.cc.cc.lib glib zlib
      ];
    in
    {
      packages = forAllSystems (pkgs: rec {
        cxssm = pkgs.rustPlatform.buildRustPackage {
          pname = "cxssm";
          inherit (cargoToml.workspace.package) version;

          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let base = baseNameOf path; in
              !(builtins.elem base [ "target" "graft" "output" "result" ".direnv" ]);
          };
          cargoLock.lockFile = ./Cargo.lock;

          cargoBuildFlags = [ "-p" "cxssm-app" "-p" "cxssm-cli" ];
          # Les tests sur le vrai bundle du jeu ne peuvent pas tourner dans le bac à sable.
          doCheck = false;

          nativeBuildInputs = with pkgs; [ pkg-config cmake makeWrapper copyDesktopItems ];
          buildInputs = (runtimeLibs pkgs) ++ (with pkgs; [ openssl zstd ]);

          desktopItems = [
            (pkgs.makeDesktopItem {
              name = "cxssm";
              desktopName = "CXSSM";
              genericName = "CarX Street Sound Mod";
              comment = "Remplacer les sons FMOD de CarX Street";
              exec = "cxssm-app %f";
              categories = [ "AudioVideo" "Audio" ];
            })
          ];

          postFixup = ''
            wrapProgram $out/bin/cxssm-app \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath (runtimeLibs pkgs)}
            wrapProgram $out/bin/cxssm \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath (runtimeLibs pkgs)}
          '';

          meta = with pkgs.lib; {
            description = "Éditeur des sons FMOD (moteur, musiques, voix) de CarX Street";
            homepage = "https://github.com/vabyz971/CXSSM";
            license = licenses.mit;
            mainProgram = "cxssm-app";
            platforms = systems;
          };
        };
        default = cxssm;
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.system}.cxssm}/bin/cxssm-app";
        };
        cli = {
          type = "app";
          program = "${self.packages.${pkgs.system}.cxssm}/bin/cxssm";
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell rec {
          nativeBuildInputs = with pkgs; [
            cargo rustc clippy rustfmt rust-analyzer
            pkg-config cmake clang
          ];
          buildInputs = (runtimeLibs pkgs) ++ (with pkgs; [ openssl zstd ]);
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      });
    };
}
