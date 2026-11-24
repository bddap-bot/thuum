param([string]$dll)
$sig = @'
[DllImport("kernel32", SetLastError=true, CharSet=CharSet.Unicode)] public static extern IntPtr LoadLibraryW(string p);
[DllImport("kernel32", SetLastError=true, CharSet=CharSet.Ansi)] public static extern IntPtr GetProcAddress(IntPtr m, string n);
'@
$k = Add-Type -MemberDefinition $sig -Name K -Namespace T -PassThru
$before = (Get-Process -Id $PID).Modules | ForEach-Object { $_.FileName }
$m = $k::LoadLibraryW("$dll")
if ($m -eq [IntPtr]::Zero) { "LoadLibraryW failed: error {0}" -f [Runtime.InteropServices.Marshal]::GetLastWin32Error(); exit 1 }
"LoadLibraryW: 0x{0:x}" -f [int64]$m
foreach ($n in "SKSEPlugin_Load","SKSEPlugin_Query","SKSEPlugin_Version","thuum_probe") { "{0}: 0x{1:x}" -f $n, [int64]$k::GetProcAddress($m, $n) }
$after = (Get-Process -Id $PID).Modules | ForEach-Object { $_.FileName }
"modules newly loaded by LoadLibraryW:"
$after | Where-Object { $before -notcontains $_ } | Sort-Object
$v = Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion"
"{0} {1} build {2}.{3}" -f (Get-CimInstance Win32_OperatingSystem).Caption, $v.DisplayVersion, $v.CurrentBuild, $v.UBR
