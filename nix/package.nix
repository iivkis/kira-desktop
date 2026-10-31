# Kira Desktop packaged for NixOS.
# Three independently cached derivations:
#   1. Crane compiles third-party Rust crates based on Cargo manifests/lock.
#   2. Crane builds Kira binaries using cached Rust artifacts.
#   3. A small final derivation assembles them with the separately cached UI.
#
# Never reference frontend in the Rust build: otherwise changing CSS would
# force a (needless) rebuild of application crates.
{
  lib,
  stdenv,
  buildNpmPackage,
  craneLib,
  pkg-config,
  wrapGAppsHook4,
  gtk4,
  gtk4-layer-shell,
  webkitgtk_6_0,
  glib,
  pipewire,
}:
let
  version = "0.1.0";

  frontend = buildNpmPackage {
    pname = "kira-desktop-frontend";
    inherit version;
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

  # cleanCargoSource excludes unrelated frontend/docs changes from the Rust
  # derivation. Crane creates a dummy source tree for buildDepsOnly, so edits
  # to .rs files do NOT invalidate compiled GTK/WebKit/GObject Rust crates.
  rustSrc = craneLib.cleanCargoSource ../.;
  commonRustArgs = {
    src = rustSrc;
    inherit version;
    strictDeps = true;
    cargoExtraArgs = "--locked --workspace";
    doCheck = false;

    nativeBuildInputs = [ pkg-config ];
    buildInputs = [
      gtk4
      gtk4-layer-shell
      webkitgtk_6_0
      glib
      pipewire
    ];
  };

  cargoArtifacts = craneLib.buildDepsOnly (commonRustArgs // {
    pname = "kira-desktop-rust-deps";
  });
  rustBinaries = craneLib.buildPackage (commonRustArgs // {
    pname = "kira-desktop-rust";
    inherit cargoArtifacts;
  });
in
# Assembly only: frontend edits rebuild THIS inexpensive derivation, not Rust.
stdenv.mkDerivation {
  pname = "kira-desktop";
  inherit version;
  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;
  doCheck = false;

  nativeBuildInputs = [ wrapGAppsHook4 ];
  buildInputs = [
    gtk4
    gtk4-layer-shell
    webkitgtk_6_0
    glib
    pipewire
  ];

  installPhase = ''
    runHook preInstall
    mkdir -p "$out/bin" "$out/share/kira-desktop/ui"
    cp "${rustBinaries}/bin/kira-core" "$out/bin/kira-core"
    cp "${rustBinaries}/bin/kira-shell" "$out/bin/kira-shell"
    chmod +x "$out/bin/kira-core" "$out/bin/kira-shell"
    cp -r "${frontend}/share/kira-desktop/ui/." "$out/share/kira-desktop/ui/"
    runHook postInstall
  '';

  # The GTK wrapper keeps its library/data setup and points WebKit to UI
  # copied into the final package.
  preFixup = ''
    gappsWrapperArgs+=(--set KIRA_UI_DIR "$out/share/kira-desktop/ui")
    # GIO owns freedesktop Exec field parsing; Core never passes commands to a shell.
    gappsWrapperArgs+=(--set KIRA_GIO_BIN "${glib}/bin/gio")
  '';

  passthru = {
    inherit cargoArtifacts rustBinaries frontend;
  };

  meta = {
    description = "Modular Niri desktop shell for NixOS";
    platforms = lib.platforms.linux;
  };
}
