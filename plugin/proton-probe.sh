#!/usr/bin/env bash
set -euo pipefail
wine=${1:?usage: proton-probe.sh <proton wine64> <thuum build dir> <output dir>}
build=$(realpath "${2:?}")
out=$(realpath -m "${3:?}")
mkdir -p "$out/run/pulse" "$out/pfx"
chmod 700 "$out/run"
export XDG_RUNTIME_DIR=$out/run PULSE_RUNTIME_PATH=$out/run/pulse PULSE_SERVER=unix:$out/run/pulse/native
export WINEPREFIX=$out/pfx WINEDEBUG=-all
libpulse=$(dirname "$(command -v pactl)")/../lib
export LD_LIBRARY_PATH=$libpulse${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}

server() {
    pulseaudio -n --daemonize=no --exit-idle-time=-1 --use-pid-file=no \
        -L "module-native-protocol-unix auth-anonymous=1 socket=$out/run/pulse/native" \
        -L "module-null-sink sink_name=micA" -L "module-null-sink sink_name=micB" >>"$out/pulse.log" 2>&1 &
    pulse=$!
    sleep 2
    pactl set-default-source micA.monitor
}

say() {
    echo "$(date +%s.%N) $1" >>"$out/events.log"
    paplay -d "$2" "$out/speech.wav"
}

server
espeak-ng -s 120 -w "$out/speech.wav" "fus ro dah. yol toor shul. wuld nah kest."
cp "$build/thuum.dll" "$build/thuum-probe.exe" "$out/"
(cd "$out" && "$wine" thuum-probe.exe 30 >"$out/probe.json") &
probe=$!
sleep 4; say "speech on source A" micA
sleep 1; echo "$(date +%s.%N) default source B" >>"$out/events.log"; pactl set-default-source micB.monitor
sleep 2; echo "$(date +%s.%N) remove source A" >>"$out/events.log"
pactl unload-module "$(pactl list short modules | awk '/sink_name=micA/{print $1}')"
sleep 1; say "speech on source B" micB
sleep 1; echo "$(date +%s.%N) audio server killed" >>"$out/events.log"; kill "$pulse"; wait "$pulse" || true
sleep 3; server; echo "$(date +%s.%N) audio server restarted" >>"$out/events.log"
sleep 3; say "speech after restart" micA
wait "$probe"
kill "$pulse"
