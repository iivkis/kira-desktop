{
  description = "Kira Desktop: NixOS-first Wayland shell built on Niri";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  # Pinned Crane release; Cargo dependencies compile in a separate derivation.
  inputs.crane.url = "github:ipetkov/crane/eb35abda9f232cc6610b1d1e3200d15c49b7ac54";

  outputs = { self, nixpkgs, crane }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f:
        nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # Expose the package when the application sources and locks exist.
      hasApplicationSources =
        builtins.pathExists ./Cargo.toml
        && builtins.pathExists ./Cargo.lock
        && builtins.pathExists ./frontend/package.json
        && builtins.pathExists ./frontend/package-lock.json;
    in
    {
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            sccache # persistent compiler cache for local cargo builds
            rustfmt
            clippy
            nodejs
            go-task
            python3
            niri
            foot # optional nested Niri terminal for interactive workspace QA
            grim # explicit visual QA screenshot capture, does not copy clipboard
            pkg-config
            gtk4
            gtk4-layer-shell
            webkitgtk_6_0
            pipewire
          ];
          # Local Cargo uses a persistent compiler cache; sandboxed nix build
          # instead reuses Crane's immutable Rust dependency derivation.
          shellHook = ''
            export RUSTC_WRAPPER="${pkgs.sccache}/bin/sccache"
            export SCCACHE_DIR="''${XDG_CACHE_HOME:-$HOME/.cache}/kira-desktop/sccache"
            export SCCACHE_CACHE_SIZE="''${SCCACHE_CACHE_SIZE:-10G}"
          '';
        };
        # Native Shell type-checking in PRs: no compositor, JS toolchain,
        # screenshot tools or interactive terminal needed. This shell uses
        # cache.nixos.org (and optional Cachix in CI) for GTK/WebKit inputs;
        # Rust artifacts are cached separately by GitHub Actions.
        ci = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            pkg-config
            gtk4
            gtk4-layer-shell
            webkitgtk_6_0
            pipewire
          ];
        };
      });

      packages = forAllSystems (pkgs:
        if hasApplicationSources
        then
          let
            app = pkgs.callPackage ./nix/package.nix { craneLib = crane.mkLib pkgs; };
          in {
            default = app;
            # Fast isolated UI build, useful for validating npmDepsHash without
            # compiling GTK/WebKit Rust dependencies.
            frontend = app.frontend;
          }
        else { }
      );

      checks = forAllSystems (pkgs: {
        repository-spec = pkgs.runCommand "kira-desktop-spec-check" { } ''
          test -s ${./docs/SPECIFICATION.md}
          test -s ${./nix/package.nix}
          test -s ${./Taskfile.yml}
          test -s ${./.github/workflows/ci.yml}
          touch "$out"
        '';
      });
    };
}
