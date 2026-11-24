import json
import sys

EXPECTED = {"reconnect": {"bursts": 5, "opens": 6}, "nomic": {"bursts": 2, "opens": 2}}
AUDIBLE_SECONDS = 2400


def bursts(rows):
    loud = [row["peak_rms"] > 3000 for row in rows]
    return sum(a and b and (i == 0 or not loud[i - 1]) for i, (a, b) in enumerate(zip(loud, loud[1:])))


def hold_failures(rows):
    fed = [i > 0 and row["samples"] - rows[i - 1]["samples"] >= 8000 for i, row in enumerate(rows)]
    return [(i, "decisions", row["owned"], row["holds"]) for i, row in enumerate(rows)
            if fed[i] and fed[i - 1] and len(row["holds"]) != row["owned"]]


def main():
    mode, path = sys.argv[1:3]
    report = json.load(open(path))
    rows = report["capture"]["per_second"]
    found = {
        "opens": rows[-1]["opens"],
        "recognizer_errors": report["capture"]["recognizer_errors"],
        "classify_median_ms_by_seconds": {h["seconds"]: round(h["median_ms"], 1) for h in report["inference"]["holds"]},
    }
    want = {"recognizer_errors": 0}
    if mode == "nohold":
        found.update(decisions=sum(len(r["holds"]) for r in rows), owned=sum(r["owned"] for r in rows),
                     stalls=rows[-1]["stalls"], audible_seconds=sum(r["peak_rms"] > 100 for r in rows))
        want.update(decisions=0, owned=0, stalls=0)
        audible_ok = found["audible_seconds"] >= AUDIBLE_SECONDS
    else:
        found.update(bursts=bursts(rows), hold_failures=hold_failures(rows),
                     release_to_decision_ms_max=max((h["release_to_decision_ms"] for r in rows for h in r["holds"]), default=None))
        want.update(EXPECTED[mode], hold_failures=[])
        audible_ok = True
        if mode == "nomic":
            found["opens_at_start"] = rows[0]["opens"]
            want["opens_at_start"] = 0
    failed = {k: (found[k], v) for k, v in want.items() if found[k] != v}
    if not audible_ok:
        failed["audible_seconds"] = (found["audible_seconds"], f">= {AUDIBLE_SECONDS}")
    print(json.dumps({"mode": mode, "found": found, "failed": failed}))
    sys.exit(1 if failed else 0)


main()
