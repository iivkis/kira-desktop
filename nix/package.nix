# Native GTK4-only package, with separately cached third-party Rust crates.
{
  lib,
  stdenv,
  craneLib,
  pkg-config,
  wrapGAppsHook4,
  writeText,
  noto-fonts,
  noto-fonts-color-emoji,
  gtk4,
  gtk4-layer-shell,
  glib,
}:
let
  version = "0.1.0";
  uiFontsConf = writeText "kira-fonts.conf" ''
    <?xml version="1.0"?>
    <!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
    <fontconfig>
      <include ignore_missing="yes">/etc/fonts/fonts.conf</include>
      <dir>${noto-fonts}/share/fonts</dir>
      <dir>${noto-fonts-color-emoji}/share/fonts</dir>
    </fontconfig>
  '';

  rustSrc = lib.cleanSourceWith {
    src = ../.;
    # Include GTK CSS embedded by kira-ui via include_str! in Rust.
    filter = path: type:
      craneLib.filterCargoSources path type
      || builtins.baseNameOf path == "style.css";
  };
  commonRustArgs = {
    src = rustSrc;
    inherit version;
    strictDeps = true;
    cargoExtraArgs = "--locked --workspace";
    doCheck = false;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ gtk4 gtk4-layer-shell glib ];
  };
  cargoArtifacts = craneLib.buildDepsOnly (commonRustArgs // {
    pname = "kira-desktop-rust-deps";
  });
  rustBinaries = craneLib.buildPackage (commonRustArgs // {
    pname = "kira-desktop-rust";
    inherit cargoArtifacts;
  });
in
stdenv.mkDerivation {
  pname = "kira-desktop";
  inherit version;
  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;
  doCheck = false;
  nativeBuildInputs = [ wrapGAppsHook4 ];
  buildInputs = [ gtk4 gtk4-layer-shell glib ];

  installPhase = ''
    runHook preInstall
    mkdir -p "$out/bin"
    cp "${rustBinaries}/bin/kira-core" "$out/bin/kira-core"
    cp "${rustBinaries}/bin/kira-shell" "$out/bin/kira-shell"
    # Acceptance-only protocol clients refuse non-isolated D-Bus; never autostarted.
    cp "${rustBinaries}/bin/kira-sni-acceptance" "$out/bin/kira-sni-acceptance"
    cp "${rustBinaries}/bin/kira-notification-acceptance" "$out/bin/kira-notification-acceptance"
    chmod +x "$out/bin/kira-core" "$out/bin/kira-shell" \
      "$out/bin/kira-sni-acceptance" "$out/bin/kira-notification-acceptance"
    runHook postInstall
  '';

  preFixup = ''
    gappsWrapperArgs+=(--set-default FONTCONFIG_FILE "${uiFontsConf}")
  '';

  passthru = { inherit cargoArtifacts rustBinaries; };
  meta = {
    description = "Native GTK4 desktop shell for Niri and NixOS";
    platforms = lib.platforms.linux;
  };
}
