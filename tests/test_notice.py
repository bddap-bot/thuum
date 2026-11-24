import re
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NOT_LINKED = {
    "autocfg 1.5.1",
    "bitflags 2.13.2",
    "block-buffer 0.10.4",
    "cc 1.5.1",
    "cpufeatures 0.2.17",
    "crunchy 0.2.4",
    "crypto-common 0.1.7",
    "derive-new 0.5.9",
    "digest 0.10.7",
    "errno 0.3.14",
    "find-msvc-tools 0.1.14",
    "generic-array 0.14.7",
    "itertools 0.10.5",
    "libc 0.2.189",
    "linux-raw-sys 0.12.1",
    "liquid-derive 0.26.10",
    "pastey 0.1.1",
    "pest_derive 2.9.2",
    "pest_generator 2.9.2",
    "pest_meta 2.9.2",
    "portable-atomic 1.15.0",
    "portable-atomic-util 0.2.8",
    "proc-macro2 1.0.107",
    "prost-derive 0.11.9",
    "quote 1.0.47",
    "redox_syscall 0.5.18",
    "rustix 1.1.5",
    "serde_derive 1.0.229",
    "sha2 0.10.9",
    "shlex 2.0.1",
    "syn 1.0.109",
    "syn 2.0.119",
    "syn 3.0.6",
    "time-macros 0.2.32",
    "tinyvec 1.13.3",
    "typenum 1.20.1",
    "unicode-ident 1.0.26",
    "unicode-normalization 0.1.25",
    "version_check 0.9.5",
    "wasi 0.11.1+wasi-snapshot-preview1",
    "windows-implement 0.60.2",
    "windows-interface 0.59.3",
    "xattr 1.6.1",
    "zerocopy-derive 0.8.59",
}


class NoticeTests(unittest.TestCase):
    def test_notice_names_every_crate_linked_into_the_dll_at_its_locked_version(self):
        notice = (ROOT / "NOTICE").read_text()
        listed = {(m[1], m[2]) for m in re.finditer(r"^(\S+) (\d\S*)  \[", notice, re.M)}
        locked = {(p["name"], p["version"]) for p in tomllib.loads((ROOT / "plugin" / "Cargo.lock").read_text())["package"]
                  if p["name"] != "thuum" and f"{p['name']} {p['version']}" not in NOT_LINKED}
        self.assertEqual(listed, locked)


if __name__ == "__main__":
    unittest.main()
