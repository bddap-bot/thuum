import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import tempfile
import tomllib
import zipfile
from pathlib import Path
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parent.parent
PLUGIN = ROOT / "plugin"
TARGET = "x86_64-pc-windows-gnu"
RUSTFLAGS = f"CARGO_TARGET_{TARGET.upper().replace('-', '_')}_RUSTFLAGS"
EPOCH = (1980, 1, 1, 0, 0, 0)
SYSTEM_DLLS = {"advapi32", "bcrypt", "bcryptprimitives", "combase", "kernel32", "msvcrt", "ntdll", "ole32",
               "oleaut32", "propsys", "rpcrt4", "shell32", "userenv", "ws2_32"}
EXPORTS = ["SKSEPlugin_Load", "SKSEPlugin_Query", "SKSEPlugin_Version"]


def classifier():
    pin = json.loads((ROOT / "data" / "hold-classifier.json").read_text())
    path = ROOT / ".scratch" / pin["file"]
    if not path.exists():
        path.parent.mkdir(exist_ok=True)
        part = path.with_suffix(".part")
        with urlopen(pin["url"], timeout=60) as response, open(part, "wb") as out:
            shutil.copyfileobj(response, out)
        data = part.read_bytes()
        if len(data) != pin["bytes"] or hashlib.sha256(data).hexdigest() != pin["sha256"]:
            part.unlink()
            raise ValueError(f"{pin['url']} does not match the pinned size and SHA-256")
        part.rename(path)
    return path


def build(stage):
    env = {k: v for k, v in os.environ.items() if k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    env |= {"THUUM_CLASSIFIER": str(classifier()), "RUSTC_WRAPPER": ""}
    cargo_home = os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))
    target = PLUGIN / os.environ.get("CARGO_TARGET_DIR", "target")
    remap = " ".join(f"--remap-path-prefix={src}={dst}"
                     for src, dst in ((cargo_home, "/cargo"), (target, "/target"), (PLUGIN, "/thuum")))
    esp = stage / "thuum.esp"
    dll = target / TARGET / "release" / "thuum.dll"
    dump = subprocess.run(["nix-shell", "shell.nix", "--run",
                           f"export {RUSTFLAGS}=\"${RUSTFLAGS}\"{shlex.quote(' ' + remap)}"
                           f" && cargo build --release --locked --lib --target {TARGET} >&2"
                           f" && cargo run --release --locked --bin thuum-esp -- {shlex.quote(str(esp))} >&2"
                           f" && x86_64-w64-mingw32-objdump -p {shlex.quote(str(dll))}"],
                          cwd=PLUGIN, env=env, check=True, stdout=subprocess.PIPE, text=True).stdout
    return dll, esp, dump


def dependencies(dump):
    imports = sorted({line.split()[-1].lower().removesuffix(".dll") for line in dump.splitlines()
                      if line.strip().startswith("DLL Name:")})
    table = dump.partition("[Ordinal/Name Pointer] Table")[2]
    exports = re.findall(r"^\s+\[\s*\d+\]\s+\+base\[\s*\d+\]\s+[0-9a-f]+\s+(\S+)$", table, re.M)
    foreign = [name for name in imports if name not in SYSTEM_DLLS and not name.startswith("api-ms-win-")]
    if foreign or exports != EXPORTS:
        raise SystemExit(f"thuum.dll imports {foreign} beyond Windows or exports {exports}")
    return {"imports": imports, "exports": exports}


def refuse_machine_paths(dll):
    found = re.findall(rb"(?:/home/|/nix/store/|/root/|/tmp/|/opt/|/usr/|/Users/|[A-Za-z]:\\Users\\)[^\x00-\x20\x7f-\xff\"',]+", Path(dll).read_bytes())
    if found:
        raise SystemExit(f"thuum.dll contains machine paths {sorted({path.decode() for path in found})}")


def layout(dll, esp):
    return {
        "thuum.esp": esp,
        "SKSE/Plugins/thuum.dll": dll,
        "SKSE/Plugins/thuum/LICENSE.txt": ROOT / "LICENSE",
        "SKSE/Plugins/thuum/NOTICE.txt": ROOT / "NOTICE",
    }


def write_archive(files, out):
    with zipfile.ZipFile(out, "w") as archive:
        for name in sorted(files):
            info = zipfile.ZipInfo(name, EPOCH)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            archive.writestr(info, Path(files[name]).read_bytes(), compresslevel=9)


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def package(out_dir):
    out_dir = Path(out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    version = tomllib.loads((PLUGIN / "Cargo.toml").read_text())["package"]["version"]
    archive = out_dir / f"thuum-{version}.zip"
    with tempfile.TemporaryDirectory() as stage:
        dll, esp, dump = build(Path(stage))
        dll_report = dependencies(dump)
        refuse_machine_paths(dll)
        files = layout(dll, esp)
        write_archive(files, archive)
        members = {name: {"bytes": Path(path).stat().st_size, "sha256": sha256(path)}
                   for name, path in sorted(files.items())}
    return {
        "archive": str(archive),
        "bytes": archive.stat().st_size,
        "sha256": sha256(archive),
        "dll": dll_report,
        "files": members,
    }
