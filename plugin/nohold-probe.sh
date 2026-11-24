#!/usr/bin/env bash
set -euo pipefail
wine=${1:?usage: nohold-probe.sh <proton wine64> <thuum probe build dir> <output dir> <mix.wav>}
build=$(realpath "${2:?}")
out=$(realpath -m "${3:?}")
mix=$(realpath "${4:?}")
here=$(dirname "$(realpath "$0")")
mkdir -p "$out/run/pulse" "$out/pfx"
chmod 700 "$out/run"
export XDG_RUNTIME_DIR=$out/run PULSE_RUNTIME_PATH=$out/run/pulse PULSE_SERVER=unix:$out/run/pulse/native
export WINEPREFIX=$out/pfx WINEDEBUG=-all
libpulse=$(dirname "$(command -v pactl)")/../lib
export LD_LIBRARY_PATH=$libpulse${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
pulseaudio -n --daemonize=no --exit-idle-time=-1 --use-pid-file=no \
    -L "module-native-protocol-unix auth-anonymous=1 socket=$out/run/pulse/native" \
    -L "module-null-sink sink_name=mic" >>"$out/pulse.log" 2>&1 &
pulse=$!
trap 'kill "$pulse" 2>/dev/null || true; WINEPREFIX=$out/pfx "$(dirname "$wine")/wineserver" -k 2>/dev/null || true' EXIT
until pactl info >/dev/null 2>&1; do sleep 0.2; done
pactl set-default-source mic.monitor
seconds=$(($(soxi -D "$mix" | cut -d. -f1) + 40))
cp "$build/thuum.dll" "$build/thuum-probe.exe" "$out/"
(cd "$out" && "$wine" thuum-probe.exe "$seconds" nohold >"$out/probe.json") &
probe=$!
until pactl list short source-outputs | grep -q .; do sleep 0.5; done
paplay -d mic "$mix"
wait "$probe"
python3 "$here/windows-probe-check.py" nohold "$out/probe.json"
