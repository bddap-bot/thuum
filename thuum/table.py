import json
import re
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Phrase:
    id: str
    shout: str
    tier: int
    words: tuple[str, ...]


def load_table(path=None):
    path = path or Path(__file__).resolve().parent.parent / "data" / "shouts.json"
    data = json.loads(Path(path).read_text())
    if data["schema"] != 1:
        raise ValueError("unsupported table schema")
    phrases = {}
    seen_words = set()
    for shout in data["shouts"]:
        name, words = shout["id"], shout["words"]
        if not re.fullmatch(r"[a-z]+(?:_[a-z]+)*", name):
            raise ValueError("invalid shout identifier")
        if len(words) != 3 or any(not re.fullmatch(r"[a-z]+", w) for w in words):
            raise ValueError("expected three dragon words")
        for tier in range(1, 4):
            key = f"{name}:{tier}"
            prefix = tuple(words[:tier])
            if key in phrases or prefix in seen_words:
                raise ValueError("duplicate or ambiguous phrase")
            phrases[key] = Phrase(key, name, tier, prefix)
            seen_words.add(prefix)
    if not phrases:
        raise ValueError("empty table")
    return phrases
