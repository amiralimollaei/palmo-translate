"""Package a native binary; GTK/libadwaita remain system dependencies."""

import argparse
from pathlib import Path
import shutil
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=["linux", "windows", "macos"], required=True)
    parser.add_argument("--arch", choices=["x86_64", "aarch64"], required=True)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    if not args.binary.is_file():
        parser.error(f"Compiled binary not found: {args.binary}")

    root = Path(__file__).resolve().parents[2]
    output = root / "dist"
    output.mkdir(exist_ok=True)
    name = f"palmo-translate-{args.platform}-{args.arch}"
    binary_name = "palmo-translate.exe" if args.platform == "windows" else "palmo-translate"
    with tempfile.TemporaryDirectory() as directory:
        package = Path(directory) / name
        package.mkdir()
        shutil.copy2(args.binary, package / binary_name)
        shutil.copy2(root / "README.md", package / "README.md")
        if args.platform == "linux":
            shutil.copy2(
                root / "data/io.github.amiralimollaei.PalmoTranslate.desktop",
                package / "io.github.amiralimollaei.PalmoTranslate.desktop",
            )
        archive = shutil.make_archive(
            str(output / name),
            "zip" if args.platform == "windows" else "gztar",
            root_dir=directory,
            base_dir=name,
        )
    print(archive)


if __name__ == "__main__":
    main()
