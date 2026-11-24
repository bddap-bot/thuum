param([string]$cmd = "list", [string]$name = "", [int]$on = 1)
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IMMDeviceEnumerator {
  [PreserveSig] int EnumAudioEndpoints(int flow, int mask, out IMMDeviceCollection devices);
  [PreserveSig] int GetDefaultAudioEndpoint(int flow, int role, out IMMDevice device);
}
[ComImport, Guid("0BD7A1BE-7A1A-44DB-8397-CC5392387B5E"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IMMDeviceCollection {
  [PreserveSig] int GetCount(out uint n);
  [PreserveSig] int Item(uint i, out IMMDevice device);
}
[ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IMMDevice {
  [PreserveSig] int Activate(ref Guid iid, int ctx, IntPtr p, [MarshalAs(UnmanagedType.IUnknown)] out object o);
  [PreserveSig] int OpenPropertyStore(int access, out IntPtr store);
  [PreserveSig] int GetId([MarshalAs(UnmanagedType.LPWStr)] out string id);
  [PreserveSig] int GetState(out int state);
}
[ComImport, Guid("f8679f50-850a-41cf-9c72-430f290290c8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IPolicyConfig {
  [PreserveSig] int GetMixFormat(IntPtr a, IntPtr b);
  [PreserveSig] int GetDeviceFormat(IntPtr a, int b, IntPtr c);
  [PreserveSig] int ResetDeviceFormat(IntPtr a);
  [PreserveSig] int SetDeviceFormat(IntPtr a, IntPtr b, IntPtr c);
  [PreserveSig] int GetProcessingPeriod(IntPtr a, int b, IntPtr c, IntPtr d);
  [PreserveSig] int SetProcessingPeriod(IntPtr a, IntPtr b);
  [PreserveSig] int GetShareMode(IntPtr a, IntPtr b);
  [PreserveSig] int SetShareMode(IntPtr a, IntPtr b);
  [PreserveSig] int GetPropertyValue(IntPtr a, IntPtr b, IntPtr c);
  [PreserveSig] int SetPropertyValue(IntPtr a, IntPtr b, IntPtr c);
  [PreserveSig] int SetDefaultEndpoint([MarshalAs(UnmanagedType.LPWStr)] string id, int role);
  [PreserveSig] int SetEndpointVisibility([MarshalAs(UnmanagedType.LPWStr)] string id, int visible);
}
[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] public class MMDeviceEnumerator {}
[ComImport, Guid("870af99c-171d-4f9e-af0d-e63df40c2bc9")] public class PolicyConfigClient {}
public static class Endpoints {
  public static string[] Capture() {
    var e = (IMMDeviceEnumerator)new MMDeviceEnumerator();
    IMMDeviceCollection c; int hr = e.EnumAudioEndpoints(1, 0xF, out c);
    if (hr != 0) throw new COMException("EnumAudioEndpoints", hr);
    uint n; c.GetCount(out n);
    var r = new string[n];
    for (uint i = 0; i < n; i++) { IMMDevice d; c.Item(i, out d); string id; d.GetId(out id); int s; d.GetState(out s); r[i] = s + " " + id; }
    return r;
  }
  public static string Default() {
    var e = (IMMDeviceEnumerator)new MMDeviceEnumerator();
    IMMDevice d; int hr = e.GetDefaultAudioEndpoint(1, 0, out d);
    if (hr != 0) return "none 0x" + hr.ToString("x8");
    string id; d.GetId(out id); return id;
  }
  public static int SetDefault(string id) {
    var p = (IPolicyConfig)new PolicyConfigClient();
    int hr = 0;
    for (int role = 0; role < 3; role++) hr |= p.SetDefaultEndpoint(id, role);
    return hr;
  }
  public static int Visible(string id, int on) { return ((IPolicyConfig)new PolicyConfigClient()).SetEndpointVisibility(id, on); }
}
'@
$names = @{}
Get-PnpDevice -Class AudioEndpoint | ForEach-Object { $names[($_.InstanceId -replace '^SWD\\MMDEVAPI\\', '').ToLower()] = $_.FriendlyName }
$ErrorActionPreference = "Stop"
function idOf($prefix) {
  $found = @($names.GetEnumerator() | Where-Object { $_.Key -like "{0.0.1.*" -and $_.Value -like "$prefix*" })
  if ($found.Count -ne 1) { throw "expected one capture endpoint named $prefix*, found $($found.Count)" }
  $found[0].Key
}
$state = @{ 1 = "active"; 2 = "disabled"; 4 = "notpresent"; 8 = "unplugged" }
switch ($cmd) {
  "list" {
    $def = [Endpoints]::Default().ToLower()
    foreach ($x in [Endpoints]::Capture()) {
      $s, $id = $x -split " ", 2
      "{0,-10} {1}{2}" -f $state[[int]$s], $names[$id.ToLower()], $(if ($id.ToLower() -eq $def) { "  [default]" } else { "" })
    }
    if ($def -like "none*") { "default capture: $def" }
  }
  "default" { $hr = [Endpoints]::SetDefault((idOf $name)); "SetDefaultEndpoint {0}: 0x{1:x8}" -f $name, $hr; if ($hr) { exit 1 } }
  "visible" { $hr = [Endpoints]::Visible((idOf $name), $on); "SetEndpointVisibility {0} {1}: 0x{2:x8}" -f $name, $on, $hr; if ($hr) { exit 1 } }
}
