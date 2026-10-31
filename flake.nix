{
  description = "Kira Desktop: NixOS-first Wayland shell built on Niri";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
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
            rustfmt
            clippy
            nodejs
            go-task
            python3
            niri
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
        then { default = pkgs.callPackage ./nix/package.nix { }; }
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
