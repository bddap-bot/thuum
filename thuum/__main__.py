import argparse
import json

from .gamedata import verify
from .package import package
from .table import load_table


def main():
    parser = argparse.ArgumentParser(description="thuum shout table, game data check and packaging")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("table")
    sub.add_parser("game-forms").add_argument("data_dir")
    sub.add_parser("package").add_argument("out_dir")
    args = parser.parse_args()
    if args.command == "table":
        result = {key: list(phrase.words) for key, phrase in load_table().items()}
    elif args.command == "game-forms":
        result = verify(args.data_dir, load_table())
    else:
        result = package(args.out_dir)
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
