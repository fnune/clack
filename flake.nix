{
  description = "Clack: a Wayland keystroke visualiser";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];

      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );

      toolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

      buildInputsFor = pkgs: [
        pkgs.cairo
        pkgs.glib
        pkgs.libxkbcommon
        pkgs.pango
      ];

      # Only the tray app needs these. The CLI package deliberately does not
      # pull in a browser engine.
      trayInputsFor = pkgs: [
        pkgs.gdk-pixbuf
        pkgs.gtk3
        pkgs.libayatana-appindicator
        pkgs.libsoup_3
        pkgs.librsvg
        pkgs.openssl
        pkgs.webkitgtk_4_1
      ];
      nativeBuildInputsFor = pkgs: [ pkgs.pkg-config ];

      clackFor =
        pkgs:
        let
          toolchain = toolchainFor pkgs;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
        in
        rustPlatform.buildRustPackage {
          pname = "clack";
          version = (nixpkgs.lib.importTOML ./Cargo.toml).workspace.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = nativeBuildInputsFor pkgs;
          buildInputs = buildInputsFor pkgs;

          postInstall = ''
            install -Dm444 packaging/65-clack.rules \
              $out/lib/udev/rules.d/65-clack.rules
          '';

          meta = {
            description = "Wayland keystroke visualiser";
            license = nixpkgs.lib.licenses.mit;
            platforms = systems;
            mainProgram = "clack";
          };
        };
    in
    {
      packages = forAllSystems (pkgs: {
        default = clackFor pkgs;
        clack = clackFor pkgs;
      });

      apps = forAllSystems (pkgs: rec {
        default = clack;
        clack = {
          type = "app";
          program = "${nixpkgs.lib.getExe (clackFor pkgs)}";
        };
      });

      # Installing the package alone is not enough: udev only reads rules that
      # a system has been told about, so a profile install leaves Clack unable
      # to open any keyboard.
      nixosModules.default =
        { config, lib, pkgs, ... }:
        let
          cfg = config.programs.clack;
        in
        {
          options.programs.clack = {
            enable = lib.mkEnableOption "Clack, a Wayland keystroke visualiser";

            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
              defaultText = lib.literalExpression "clack.packages.\${system}.default";
              description = "Which build of Clack to install.";
            };
          };

          config = lib.mkIf cfg.enable {
            environment.systemPackages = [ cfg.package ];
            services.udev.packages = [ cfg.package ];
          };
        };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = [
            (toolchainFor pkgs)
            pkgs.cargo-deny
            pkgs.cargo-nextest
            pkgs.cargo-tauri
            pkgs.evtest
            pkgs.grim
            pkgs.imagemagick
            pkgs.nodejs_24
            pkgs.pnpm
            pkgs.sway
            pkgs.wayland-utils
          ]
          ++ nativeBuildInputsFor pkgs
          ++ buildInputsFor pkgs
          ++ trayInputsFor pkgs;

          # The tray indicator library is loaded by name at runtime, so it has
          # to be findable rather than merely linked against. Only the tray
          # wants this: putting it on LD_LIBRARY_PATH for the whole shell puts
          # Nix's glibc ahead of the host's for every subprocess, which breaks
          # host tools such as git.
          shellHook = ''
            export CLACK_TRAY_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (trayInputsFor pkgs)}"
          '';
        };
      });

      checks = forAllSystems (pkgs: { clack = clackFor pkgs; });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
