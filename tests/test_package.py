import tempfile
import unittest
import zipfile
from pathlib import Path

from thuum.package import dependencies, layout, refuse_machine_paths, sha256, write_archive

DUMP = """
\tDLL Name: KERNEL32.dll
\tDLL Name: api-ms-win-core-synch-l1-2-0.dll
\tDLL Name: ole32.dll

[Ordinal/Name Pointer] Table -- Ordinal Base 1
\t          Ordinal   Hint Name
\t[   0] +base[   1]  0000 SKSEPlugin_Load
\t[   1] +base[   2]  0001 SKSEPlugin_Query
\t[   2] +base[   3]  0002 SKSEPlugin_Version

The Function Table (interpreted .pdata section contents)
"""


class PackageTests(unittest.TestCase):
    def test_archive_is_a_data_folder_and_byte_identical_across_runs(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "thuum.dll").write_bytes(b"dll")
            (tmp / "thuum.esp").write_bytes(b"esp")
            files = layout(tmp / "thuum.dll", tmp / "thuum.esp")
            write_archive(files, tmp / "a.zip")
            (tmp / "thuum.esp").touch()
            write_archive(files, tmp / "b.zip")
            self.assertEqual(sha256(tmp / "a.zip"), sha256(tmp / "b.zip"))
            with zipfile.ZipFile(tmp / "a.zip") as archive:
                self.assertEqual(archive.namelist(), ["SKSE/Plugins/thuum.dll", "SKSE/Plugins/thuum/LICENSE.txt",
                                                      "SKSE/Plugins/thuum/NOTICE.txt", "thuum.esp"])
                self.assertEqual(archive.read("SKSE/Plugins/thuum.dll"), b"dll")
                self.assertIn(b"MIT License", archive.read("SKSE/Plugins/thuum/LICENSE.txt"))

    def test_dependencies_accept_only_windows_imports_and_the_skse_exports(self):
        self.assertEqual(dependencies(DUMP), {"imports": ["api-ms-win-core-synch-l1-2-0", "kernel32", "ole32"],
                                              "exports": ["SKSEPlugin_Load", "SKSEPlugin_Query", "SKSEPlugin_Version"]})
        with self.assertRaises(SystemExit):
            dependencies(DUMP.replace("ole32.dll", "onnxruntime.dll"))
        with self.assertRaises(SystemExit):
            dependencies(DUMP.split("[Ordinal")[0])

    def test_a_dll_containing_a_machine_path_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            dll = Path(tmp) / "thuum.dll"
            dll.write_bytes(b"\x00/thuum/src/lib.rs\x00/cargo/registry/src\x00")
            refuse_machine_paths(dll)
            for path in (b"/home/someone/venv/torch/nn/modules/conv.py", b"/tmp/build/conv.py",
                         b"C:\\Users\\someone\\torch\\conv.py"):
                dll.write_bytes(b"\x00File \"" + path + b"\", line 1\x00")
                with self.assertRaisesRegex(SystemExit, "conv.py"):
                    refuse_machine_paths(dll)


if __name__ == "__main__":
    unittest.main()
