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
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

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

      # Only the desktop app needs these. The CLI package deliberately does not
      # pull in a browser engine.
      desktopInputsFor = pkgs: [
        pkgs.gdk-pixbuf
        pkgs.gtk3
        pkgs.libayatana-appindicator
        pkgs.libsoup_3
        pkgs.librsvg
        pkgs.openssl
        pkgs.webkitgtk_4_1
      ];
      nativeBuildInputsFor = pkgs: [ pkgs.pkg-config ];

      desktopRelease = {
        version = "0.1.0";
        hash = "sha256-wpdJLzuocfJ7mMEPhZaAYc1m6vJT2rG9FiiT6TlT9hY=";
      };

      desktopFor =
        pkgs:
        let
          src = pkgs.fetchurl {
            url = "https://github.com/fnune/clack/releases/download/v${desktopRelease.version}/Clack-x86_64.AppImage";
            inherit (desktopRelease) hash;
          };

          icons = {
            "32x32" = ./apps/desktop/src-tauri/icons/32x32.png;
            "128x128" = ./apps/desktop/src-tauri/icons/128x128.png;
            "512x512" = ./apps/desktop/src-tauri/icons/icon.png;
          };
        in
        pkgs.appimageTools.wrapType2 {
          pname = "clack-desktop";
          inherit (desktopRelease) version;
          inherit src;

          extraInstallCommands = ''
            install -Dm444 ${./packaging/clack.desktop} \
              $out/share/applications/clack.desktop

            ${nixpkgs.lib.concatStringsSep "\n" (
              nixpkgs.lib.mapAttrsToList (size: icon: ''
                install -Dm444 ${icon} $out/share/icons/hicolor/${size}/apps/clack.png
              '') icons
            )}
          '';

          meta = {
            description = "Wayland keystroke visualiser, with a tray and a settings window";
            license = nixpkgs.lib.licenses.mit;
            platforms = [ "x86_64-linux" ];
            mainProgram = "clack-desktop";
          };
        };

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
        clack-desktop = desktopFor pkgs;
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
        {
          config,
          lib,
          pkgs,
          ...
        }:
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

      homeManagerModules.default =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          cfg = config.programs.clack;
          toml = pkgs.formats.toml { };

          withoutUnset =
            attrs:
            lib.filterAttrs (_: value: value != null) (
              lib.mapAttrs (
                _: value: if lib.isAttrs value && !lib.isDerivation value then withoutUnset value else value
              ) attrs
            );

          settings = lib.filterAttrs (_: section: section != { }) (withoutUnset cfg.settings);
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

            desktop = {
              enable = lib.mkEnableOption ''
                the desktop app: a tray icon and a settings window, installed from
                the AppImage published with the release. It reads the same settings
                as the command line
              '';

              package = lib.mkOption {
                type = lib.types.package;
                default = self.packages.${pkgs.stdenv.hostPlatform.system}.clack-desktop;
                defaultText = lib.literalExpression "clack.packages.\${system}.clack-desktop";
                description = "Which build of the desktop app to install.";
              };
            };

            settings = lib.mkOption {
              default = { };
              description = ''
                What Clack reads from `$XDG_CONFIG_HOME/clack/config.toml`. Anything
                left unset is left out of the file, so Clack's own default applies.

                The file this generates belongs to the store and cannot be written
                to, so the desktop app says as much and treats a change made in its
                window as lasting until it quits.
              '';
              type = lib.types.submodule {
                freeformType = toml.type;

                options.overlay = lib.mkOption {
                  default = { };
                  description = "How the overlay is drawn.";
                  type = lib.types.submodule {
                    freeformType = toml.type;

                    options = {
                      font = lib.mkOption {
                        type = lib.types.nullOr lib.types.str;
                        default = null;
                        example = "Iosevka Bold 30";
                        description = "The Pango font description the keys are drawn in.";
                      };

                      margin = lib.mkOption {
                        type = lib.types.nullOr lib.types.ints.unsigned;
                        default = null;
                        example = 96;
                        description = "Distance from the anchored edge, in logical pixels.";
                      };

                      anchor = lib.mkOption {
                        type = lib.types.nullOr (
                          lib.types.enum [
                            "top"
                            "bottom"
                          ]
                        );
                        default = null;
                        description = "Which edge of the screen the overlay sits against.";
                      };

                      timeout_seconds = lib.mkOption {
                        type = lib.types.nullOr (lib.types.either lib.types.float lib.types.ints.unsigned);
                        default = null;
                        example = 1.5;
                        description = "How long a keystroke stays on screen once typing stops.";
                      };

                      symbols = lib.mkOption {
                        type = lib.types.nullOr lib.types.bool;
                        default = null;
                        description = "Whether keys with no letter of their own are drawn as glyphs.";
                      };

                      symbol_overrides = lib.mkOption {
                        type = lib.types.nullOr (lib.types.attrsOf lib.types.str);
                        default = null;
                        example = lib.literalExpression ''{ Tab = "⇥"; }'';
                        description = ''
                          Replacements for single keys, by keysym name. An empty string
                          hides one.
                        '';
                      };
                    };
                  };
                };

                options.desktop = lib.mkOption {
                  default = { };
                  description = "How the desktop app behaves.";
                  type = lib.types.submodule {
                    freeformType = toml.type;

                    options = {
                      start_hidden = lib.mkOption {
                        type = lib.types.nullOr lib.types.bool;
                        default = null;
                        description = "Start in the tray, without opening the settings window.";
                      };

                      start_overlay = lib.mkOption {
                        type = lib.types.nullOr lib.types.bool;
                        default = null;
                        description = "Start showing keystrokes as soon as the app opens.";
                      };

                      decorations = lib.mkOption {
                        type = lib.types.nullOr lib.types.bool;
                        default = null;
                        description = "Whether the window wears the desktop's own title bar.";
                      };
                    };
                  };
                };
              };
            };
          };

          config = lib.mkIf cfg.enable {
            home.packages = [ cfg.package ] ++ lib.optional cfg.desktop.enable cfg.desktop.package;

            xdg.configFile."clack/config.toml" = lib.mkIf (settings != { }) {
              source = toml.generate "clack-config.toml" settings;
            };
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
            pkgs.git-cliff
            pkgs.grim
            pkgs.imagemagick
            pkgs.just
            pkgs.nodejs_24
            pkgs.pnpm
            pkgs.sway
            pkgs.wayland-utils
          ]
          ++ nativeBuildInputsFor pkgs
          ++ buildInputsFor pkgs
          ++ desktopInputsFor pkgs;

          # The tray indicator library is loaded by name at runtime, so it has
          # to be findable rather than merely linked against. Only the desktop
          # app wants this: putting it on LD_LIBRARY_PATH for the whole shell puts
          # Nix's glibc ahead of the host's for every subprocess, which breaks
          # host tools such as git.
          shellHook = ''
            export CLACK_DESKTOP_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (desktopInputsFor pkgs)}"
          '';
        };
      });

      checks = forAllSystems (pkgs: {
        clack = clackFor pkgs;
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
