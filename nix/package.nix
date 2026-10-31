# NixOS package scaffold for the future Rust Core/Shell + Vite frontend.
# Not buildable until Cargo Workspace, frontend/package-lock.json and binaries exist.
# The frontend dependency hash was computed by the Nix fixed-output sandbox.
{
  lib,
  rustPlatform,
  buildNpmPackage,
  pkg-config,
  wrapGAppsHook4,
  gtk4,
  gtk4-layer-shell,
  webkitgtk_6_0,
  glib,
  pipewire,
}:
let
  frontend = buildNpmPackage {
    pname = "kira-desktop-frontend";
    version = "0.1.0";
    src = ../frontend;

    npmDepsHash = "sha256-1IUE4ratidBWc+ZT2v/Ke0lA3MnmVrPdYz153s6uqps=";
    npmBuildScript = "build";

    installPhase = ''
      runHook preInstall
      mkdir -p "$out/share/kira-desktop/ui"
      cp -r dist/. "$out/share/kira-desktop/ui/"
      runHook postInstall
    '';
  };
in
rustPlatform.buildRustPackage {
  pname = "kira-desktop";
  version = "0.1.0";
  src = lib.cleanSource ../.;

  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [ pkg-config wrapGAppsHook4 ];
  buildInputs = [
    gtk4
    gtk4-layer-shell
    webkitgtk_6_0
    glib
    pipewire
  ];

  postInstall = ''
    mkdir -p "$out/share/kira-desktop/ui"
    cp -r "${frontend}/share/kira-desktop/ui/." "$out/share/kira-desktop/ui/"
  '';

  # wrapGAppsHook4 exposes the packaged static UI directory at runtime.
  preFixup = ''
    gappsWrapperArgs+=(--set KIRA_UI_DIR "$out/share/kira-desktop/ui")
  '';

  meta = {
    description = "Modular Niri desktop shell for NixOS";
    platforms = lib.platforms.linux;
  };
}
