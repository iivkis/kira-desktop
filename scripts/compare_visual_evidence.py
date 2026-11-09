#!/usr/bin/env python3
"""Create an UNREVIEWED main/native side-by-side from real frame manifests.

Manual reviewers must supply a genuine main capture from the frozen source
and a native GTK capture of the same stimulus. A filename and manifest
cannot independently prove their process provenance or motion parity.
"""
from __future__ import annotations

import argparse
import html
import json
from pathlib import Path
import shutil
import sys
import tempfile

from dev_preview import PreviewError
from validate_nested_evidence import MAIN_SHA, png_size, validate_capture


def validate_pair(main: Path, native: Path) -> tuple[dict, dict]:
    if main.resolve() == native.resolve():
        raise PreviewError("The source and native captures cannot be the same file")
    for manifest in (main, native):
        if not manifest.is_file() or manifest.is_symlink():
            raise PreviewError("Capture inputs must be genuine regular manifest files")
    source = validate_capture(main, main.parent)
    target = validate_capture(native, native.parent)
    if source["label"] != target["label"]:
        raise PreviewError("Only captures of the same named stimulus can be compared")
    if source["dimensions"] != target["dimensions"]:
        raise PreviewError("Cannot pair screenshots with different pixel viewport sizes")
    if len(source["frames"]) != len(target["frames"]):
        raise PreviewError("Unequal frame counts require a new matched capture")
    return source, target


def comparison_html(label: str, dimensions: tuple[int, int],
                    pairs: list[dict]) -> str:
    # Every filename comes from validated_capture and is copied to a new,
    # self-contained artifact. No arbitrary paths, scripts or remote assets.
    rows = []
    for i, pair in enumerate(pairs):
        row = (
            '<section class="pair"><h2>Кадр ' + str(i) + '</h2><div class="columns">'
            '<figure><img src="' + html.escape(pair["main"]["file"], quote=True)
            + '" alt="Frozen main frame ' + str(i) + '"><figcaption>main: '
            + html.escape(str(pair["main"]["elapsed_ms"])) + ' мс</figcaption></figure>'
            '<figure><img src="' + html.escape(pair["native"]["file"], quote=True)
            + '" alt="Native GTK frame ' + str(i) + '"><figcaption>native: '
            + html.escape(str(pair["native"]["elapsed_ms"])) + ' мс</figcaption></figure>'
            '</div></section>'
        )
        rows.append(row)
    return (
        '<!doctype html><html lang="ru"><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width">'
        '<title>Kira source/native comparison — UNREVIEWED</title>'
        '<style>body{font:14px system-ui;background:#000;color:#fff;padding:18px}'
        'h1{color:#fca311}.pair{border-top:1px solid #14213d;padding:12px 0}'
        '.columns{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px}'
        'figure{margin:0;min-width:0}img{display:block;width:100%;height:auto}'
        'figcaption{padding:8px;color:#e5e5e5}'
        '@media(max-width:650px){.columns{grid-template-columns:1fr}}</style>'
        '<h1>UNREVIEWED: frozen main ↔ native GTK4</h1>'
        '<p>Stimulus: ' + html.escape(label) + ' · '
        + str(dimensions[0]) + ' × ' + str(dimensions[1])
        + ' real pixels. Both streams have independently measured capture times.</p>'
        '<p>Manual source-process attestation, same fonts/scale, keyboard/focus '
        'tests, source-motion comparison and human acceptance remain required. '
        'This index does not certify a widget or update the parity ledger.</p>'
        + "".join(rows) + '</html>'
    )


def write_comparison(main: Path, native: Path, output: Path) -> Path:
    source, target = validate_pair(main, native)
    if output.exists() or output.is_symlink():
        raise PreviewError("Refusing to overwrite existing paired evidence")
    output.parent.mkdir(parents=True, exist_ok=True)
    pairs = []
    # Stage all files then atomically publish one immutable review directory.
    with tempfile.TemporaryDirectory(
        prefix="kira-side-by-side-", dir=output.parent,
    ) as temporary:
        staged = Path(temporary)
        for role in ("main", "native"):
            (staged / role).mkdir(mode=0o700)
        for i, (source_frame, target_frame) in enumerate(
            zip(source["frames"], target["frames"], strict=True)
        ):
            pair = {}
            for role, original, frame in (
                ("main", main, source_frame),
                ("native", native, target_frame),
            ):
                filename = frame["file"]
                origin = original.parent / filename
                destination = staged / role / filename
                shutil.copyfile(origin, destination)
                if png_size(destination) != tuple(source["dimensions"]):
                    raise PreviewError("Copied frame changed or is not an intact PNG")
                pair[role] = {
                    "file": role + "/" + filename,
                    "elapsed_ms": frame["elapsed_ms"],
                }
            pairs.append(pair)
        data = {
            "status": "unreviewed",
            "source_baseline": MAIN_SHA,
            "scenario": source["label"],
            "viewport": list(source["dimensions"]),
            "frames": pairs,
            "limitations": [
                "Main capture origin is a manual assertion, not process-attested",
                "Frame timing is capture time, not compositor presentation time",
                "No pixel or TENET motion parity verdict",
            ],
        }
        (staged / "comparison-manifest.json").write_text(
            json.dumps(data, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        (staged / "index.html").write_text(
            comparison_html(source["label"], source["dimensions"], pairs),
            encoding="utf-8",
        )
        if output.exists() or output.is_symlink():
            raise PreviewError("Paired evidence destination was created concurrently")
        staged.rename(output)
    return output / "index.html"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--main-manifest", required=True, type=Path)
    parser.add_argument("--native-manifest", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        index = write_comparison(
            args.main_manifest.absolute(), args.native_manifest.absolute(),
            args.output.absolute(),
        )
        print("[comparison] " + str(index) + " (UNREVIEWED, no parity claim)")
        return 0
    except (OSError, ValueError, PreviewError) as error:
        print("[comparison] " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
