#!/usr/bin/env bash
# shellcheck disable=SC2016
set -euo pipefail
guest=${1:?usage: windows-probe.sh <ssh guest> <thuum probe build dir> <output dir> [reconnect|nomic]}
build=$(realpath "${2:?}")
out=$(realpath -m "${3:?}")
mode=${4:-reconnect}
here=$(dirname "$(realpath "$0")")
mkdir -p "$out"
ev() { echo "$(date +%s.%N) $*" >>"$out/events.log"; }
say() { ev "$1"; paplay -d "$2" "$out/speech.wav"; }
g() { ssh -o BatchMode=yes "$guest" "$@" 2>>"$out/ssh.err" | tr -d '\r'; }
au() { ev "$*"; { g "C:\\thuum\\windows-audio.ps1 $*"; g 'C:\thuum\windows-audio.ps1 list'; echo; } >>"$out/guest.log"; }
restore() {
    g 'Stop-Process -Force -Name thuum-probe -ErrorAction SilentlyContinue; schtasks /delete /tn thuumprobe /f' >/dev/null || true
    for name in Microphone Line; do g "C:\\thuum\\windows-audio.ps1 visible $name 1" >/dev/null || true; done
    g 'C:\thuum\windows-audio.ps1 default Microphone' >/dev/null || true
}

espeak-ng -s 120 -w "$out/speech.wav" "fus ro dah. yol toor shul. wuld nah kest."
g 'New-Item -Force -ItemType Directory C:\thuum | Out-Null; Set-ExecutionPolicy -Scope CurrentUser Bypass -Force; Stop-Process -Force -Name thuum-probe -ErrorAction SilentlyContinue; Start-Sleep 1; Remove-Item -Force -ErrorAction SilentlyContinue C:\thuum\probe.json,C:\thuum\done.txt; schtasks /delete /tn thuumprobe /f 2>$null; $null'
scp -q -o BatchMode=yes "$build/thuum.dll" "$build/thuum-probe.exe" "$here/windows-audio.ps1" "$guest:C:/thuum/"
trap restore EXIT
(cd "$build" && sha256sum thuum.dll thuum-probe.exe) | cut -d' ' -f1 >"$out/sha256.txt"
g 'Get-FileHash C:\thuum\thuum.dll,C:\thuum\thuum-probe.exe | % { $_.Hash.ToLower() }' >"$out/guest-sha256.txt"
cmp "$out/sha256.txt" "$out/guest-sha256.txt"
g '$os = Get-CimInstance Win32_OperatingSystem; $v = Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion"; "{0} {1} build {2}" -f $os.Caption, $v.DisplayVersion, $os.Version' >"$out/guest.log"
if [ "$mode" = nomic ]; then
    au visible Microphone 0; au visible Line 0
fi
g 'schtasks /create /tn thuumprobe /sc once /st 23:59 /it /f /tr "cmd /c C:\thuum\thuum-probe.exe 120 > C:\thuum\probe.json & echo done > C:\thuum\done.txt" | Out-Null; schtasks /run /tn thuumprobe | Out-Null'
ev "probe started"
wait_for() {
    local deadline=$((SECONDS + 120))
    until g "$1" | grep -q True; do
        [ $SECONDS -lt $deadline ] || { echo "timed out waiting for: $1" >&2; exit 1; }
        sleep 1
    done
}
wait_for '[bool](Get-Process thuum-probe -ErrorAction SilentlyContinue)'
if [ "$mode" = reconnect ]; then
    wait_for '(Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\NonPackaged\C:#thuum#thuum-probe.exe" -ErrorAction SilentlyContinue).LastUsedTimeStop -eq 0'
fi
ev "capture running"
g '"thuum-probe.exe in session {0}, the desktop (explorer.exe) in session {1}, ssh in session {2}" -f (Get-Process thuum-probe).SessionId, (Get-Process explorer).SessionId, (Get-Process -Id $PID).SessionId' >>"$out/guest.log"
if [ "$mode" = nomic ]; then
    sleep 3; au visible Line 1
    sleep 4; say "speech on Line In (micB)" micB
    sleep 3; au visible Microphone 1
    sleep 3; au default Microphone
    sleep 4; say "speech on Microphone (micA)" micA
else
    sleep 1; say "speech on Microphone (micA)" micA
    sleep 2; au visible Microphone 0
    sleep 3; say "speech on micA while Microphone disabled" micA
    sleep 2; say "speech on Line In (micB)" micB
    sleep 2; au visible Microphone 1
    sleep 4; say "speech on micA after re-enable" micA
    sleep 2; say "speech on micB after re-enable" micB
    sleep 2; au default Line
    sleep 3; say "speech on micB after default switch" micB
    sleep 2; au default Microphone
    sleep 2; ev "restart Windows Audio service"; g 'Restart-Service audiosrv -Force; (Get-Service audiosrv).Status' >>"$out/guest.log"
    sleep 5; say "speech on micA after audio service restart" micA
fi
wait_for 'Test-Path C:\thuum\done.txt'
ev "probe exited"
scp -q -o BatchMode=yes "$guest:C:/thuum/probe.json" "$out/probe.json"
python3 "$here/windows-probe-check.py" "$mode" "$out/probe.json"
