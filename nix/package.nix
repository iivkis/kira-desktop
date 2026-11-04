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
  writeText,
  noto-fonts,
  noto-fonts-color-emoji,
  gtk4,
  gtk4-layer-shell,
  webkitgtk_6_0,
  glib,
  pipewire,
}:
let
  version = "0.1.0";
  # Preserve the NixOS host font profile and users' installed glyph fallbacks.
  # A Fontconfig profile containing ONLY Noto Sans would make CJK/emoji and
  # other installed fonts disappear inside GTK/WebKit. Add the bundled families
  # after including host configuration, scoped to this binary's wrapper.
  uiFontsConf = writeText "kira-fonts.conf" ''
    <?xml version="1.0"?>
    <!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
    <fontconfig>
      <include ignore_missing="yes">/etc/fonts/fonts.conf</include>
      <dir>${noto-fonts}/share/fonts</dir>
      <dir>${noto-fonts-color-emoji}/share/fonts</dir>
    </fontconfig>
  '';

  frontend = buildNpmPackage {
    pname = "kira-desktop-frontend";
    inherit version;
    src = ../frontend;

    npmDepsHash = "sha256-MlZUZJG1+rYRn+IZRMN/XAT1sYRTO8bz1vC9URffmCs=";
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
  # Every widget is rendered in the independently packaged Svelte bundle;
  # native Rust sources contain no GTK presentation CSS.
  rustSrc = lib.cleanSourceWith {
    src = ../.;
    filter = craneLib.filterCargoSources;
  };
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
    gappsWrapperArgs+=(--set-default FONTCONFIG_FILE "${uiFontsConf}")
  '';

  passthru = {
    inherit cargoArtifacts rustBinaries frontend;
  };

  meta = {
    description = "Modular Niri desktop shell for NixOS";
    platforms = lib.platforms.linux;
  };
}
