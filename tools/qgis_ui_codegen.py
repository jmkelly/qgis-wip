#!/usr/bin/env python3
"""Codegen stub for QGIS .ui files -> Rust widget modules."""
import argparse, glob, os, sys

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("input_dir", nargs="?", default=".")
    ap.add_argument("--out", default="gui/src/generated")
    args = ap.parse_args()
    files = glob.glob(os.path.join(args.input_dir, "**", "*.ui"), recursive=True)
    print(f"found {len(files)} .ui files")
    os.makedirs(args.out, exist_ok=True)
    # No real codegen yet; write a placeholder index.
    with open(os.path.join(args.out, "mod.rs"), "w") as f:
        f.write("// Generated placeholder. Implement codegen later.\n")
    return 0

if __name__ == "__main__":
    sys.exit(main())
