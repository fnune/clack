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

      buildInputsFor = pkgs: [ pkgs.libxkbcommon ];
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
            license = nixpkgs.lib.licenses.gpl3Plus;
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

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = [
            (toolchainFor pkgs)
            pkgs.cargo-deny
            pkgs.cargo-nextest
            pkgs.evtest
            pkgs.wayland-utils
          ]
          ++ nativeBuildInputsFor pkgs
          ++ buildInputsFor pkgs;
        };
      });

      checks = forAllSystems (pkgs: { clack = clackFor pkgs; });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
