{
  description = "Kira Desktop: Rust-native GTK4 Niri/Wayland shell";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.crane.url = "github:ipetkov/crane/eb35abda9f232cc6610b1d1e3200d15c49b7ac54";

  outputs = { self, nixpkgs, crane }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f:
        nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      hasApplicationSources =
        builtins.pathExists ./Cargo.toml
        && builtins.pathExists ./Cargo.lock;
    in
    {
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo rustc sccache rustfmt clippy go-task python3
            niri foot grim dbus glib pkg-config gtk4 gtk4-layer-shell
          ];
          shellHook = ''
            export RUSTC_WRAPPER="${pkgs.sccache}/bin/sccache"
            export SCCACHE_DIR="''${XDG_CACHE_HOME:-$HOME/.cache}/kira-desktop/sccache"
            export SCCACHE_CACHE_SIZE="''${SCCACHE_CACHE_SIZE:-10G}"
          '';
        };
        ci = pkgs.mkShell {
          packages = with pkgs; [
            cargo rustc clippy python3 dbus glib pkg-config gtk4 gtk4-layer-shell
          ];
        };
        graphical-ci = pkgs.mkShell {
          # Weston hosts genuine Niri/GTK without DRM or host user sessions.
          packages = with pkgs; [
            (python3.withPackages (ps: [ ps.pillow ])) weston niri grim dbus glib mesa libglvnd
          ];
          # Niri's windowed backend needs EGL; this non-NixOS CI host
          # cannot discover Nix Mesa/GLVND without explicit search paths.
          shellHook = ''
            export __EGL_VENDOR_LIBRARY_FILENAMES="${pkgs.mesa}/share/glvnd/egl_vendor.d/50_mesa.json"
            export LIBGL_DRIVERS_PATH="${pkgs.mesa}/lib/dri"
            export LD_LIBRARY_PATH="${pkgs.libglvnd}/lib:${pkgs.mesa}/lib"
            export LIBGL_ALWAYS_SOFTWARE=1
            export GALLIUM_DRIVER=llvmpipe
          '';
        };
      });

      packages = forAllSystems (pkgs:
        if hasApplicationSources then {
          default = pkgs.callPackage ./nix/package.nix { craneLib = crane.mkLib pkgs; };
        } else { });

      checks = forAllSystems (pkgs: {
        repository-spec = pkgs.runCommand "kira-native-spec-check" { } ''
          test -s ${./docs/ARCHITECTURE_NATIVE.md}
          test -s ${./docs/MIGRATION_NATIVE_GTK.md}
          test -s ${./crates/kira-ui/src/lib.rs}
          test -s ${./crates/kira-ui/src/style.css}
          test -s ${./crates/kira-services/src/lib.rs}
          test -s ${./crates/kira-shell/src/main.rs}
          touch "$out"
        '';
      });
    };
}
