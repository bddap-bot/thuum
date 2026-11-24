import json
import struct
import zlib
from pathlib import Path

FORMS = Path(__file__).resolve().parent.parent / "data" / "game-forms.json"


def load_forms(path=FORMS):
    data = json.loads(Path(path).read_text())
    if data["schema"] != 1:
        raise ValueError("unsupported game form schema")
    return {k: (v["file"], int(v["id"], 16)) for k, v in data["shouts"].items()}


def _records(data, want):
    i = 0
    while i < len(data):
        kind = data[i:i + 4]
        size, = struct.unpack_from("<I", data, i + 4)
        if kind == b"GRUP":
            yield from _records(data[i + 24:i + size], want)
            i += size
            continue
        flags, form = struct.unpack_from("<II", data, i + 8)
        body = data[i + 24:i + 24 + size]
        i += 24 + size
        if kind not in want:
            continue
        if flags & 0x40000:
            body = zlib.decompress(body[4:])
        fields, j = [], 0
        while j < len(body):
            length, = struct.unpack_from("<H", body, j + 4)
            fields.append((body[j:j + 4], body[j + 6:j + 6 + length]))
            j += 6 + length
        yield kind, form, fields


def shout_words(data_dir, file):
    found = {}
    for kind, form, fields in _records((Path(data_dir) / file).read_bytes(), {b"SHOU"}):
        found[form & 0xFFFFFF] = [struct.unpack_from("<I", v)[0] for k, v in fields if k == b"SNAM"]
    return found


def verify(data_dir, table):
    forms = load_forms()
    if set(forms) != {p.shout for p in table.values()}:
        raise ValueError("game forms must cover exactly the shout table")
    records = {f: shout_words(data_dir, f) for f in {f for f, _ in forms.values()}}
    result = {}
    for shout, (file, local) in sorted(forms.items()):
        words = records[file].get(local)
        if words is None or len(words) != 3 or len(set(words)) != 3 or 0 in words:
            raise ValueError(f"{shout}: {file}:{local:06X} is not a three-word shout")
        result[shout] = f"{file}:{local:06X}"
    return result
