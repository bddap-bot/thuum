{ pkgs ? import <nixpkgs> { } }:
let
  mingw = pkgs.pkgsCross.mingwW64;
in
pkgs.mkShell {
  packages = [ pkgs.rustup mingw.stdenv.cc ];
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "x86_64-w64-mingw32-gcc";
  CC_x86_64_pc_windows_gnu = "x86_64-w64-mingw32-gcc";
  AR_x86_64_pc_windows_gnu = "x86_64-w64-mingw32-ar";
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-C target-feature=+crt-static -L ${mingw.windows.pthreads}/lib";
}
