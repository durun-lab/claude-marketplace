# usage: watch.ps1 <log> <stopfile> [maxSeconds]
# コンソールの窓が表に出る・前に出るイベントを WinEvent のフックで拾い、ログに書く。
# 10 秒に 6 個以上出たら Bazel の java を止める（ユーザーのフォーカスを守る安全装置）。
param([string]$Log, [string]$StopFile, [int]$MaxSeconds = 1800)

$src = @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class ConsoleWatch {
  delegate void WinEventDelegate(IntPtr hook, uint ev, IntPtr hwnd, int idObject, int idChild, uint thread, uint time);
  [StructLayout(LayoutKind.Sequential)] struct MSG { public IntPtr h; public uint m; public IntPtr w; public IntPtr l; public uint t; public int x; public int y; }
  [DllImport("user32.dll")] static extern IntPtr SetWinEventHook(uint min, uint max, IntPtr mod, WinEventDelegate p, uint pid, uint tid, uint flags);
  [DllImport("user32.dll")] static extern bool UnhookWinEvent(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder sb, int n);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool PeekMessage(out MSG m, IntPtr h, uint a, uint b, uint r);
  [DllImport("user32.dll")] static extern bool TranslateMessage(ref MSG m);
  [DllImport("user32.dll")] static extern IntPtr DispatchMessage(ref MSG m);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("kernel32.dll")] static extern IntPtr GetConsoleWindow();

  static readonly string[] Classes = { "ConsoleWindowClass", "CASCADIA_HOSTING_WINDOW_CLASS", "PseudoConsoleWindow" };
  static StreamWriter log;
  static List<DateTime> hits = new List<DateTime>();
  static int total;
  static bool tripped;
  static WinEventDelegate keep;

  static string Proc(uint pid) { try { return Process.GetProcessById((int)pid).ProcessName; } catch { return "?"; } }

  static void OnEvent(IntPtr hook, uint ev, IntPtr hwnd, int idObject, int idChild, uint thread, uint time) {
    if (idObject != 0 || hwnd == IntPtr.Zero) return;
    var sb = new StringBuilder(256);
    GetClassName(hwnd, sb, 256);
    string cls = sb.ToString();
    uint pid; GetWindowThreadProcessId(hwnd, out pid);
    bool console = Array.IndexOf(Classes, cls) >= 0;
    string name = ev == 3 ? "FOREGROUND" : "SHOW";
    if (!console && ev != 3) return;
    if (ev == 3 && !console && !IsWindowVisible(hwnd)) return;
    log.WriteLine("{0:HH:mm:ss.fff} {1} class={2} pid={3} proc={4} visible={5}", DateTime.Now, name, cls, pid, Proc(pid), IsWindowVisible(hwnd));
    log.Flush();
    if (!console) return;
    total++;
    var now = DateTime.Now;
    hits.Add(now);
    hits.RemoveAll(t => (now - t).TotalSeconds > 10);
    if (hits.Count > 5 && !tripped) { tripped = true; Trip(); }
  }

  static void Trip() {
    log.WriteLine("TRIP: more than 5 console windows within 10 s; killing Bazel servers");
    foreach (var p in Process.GetProcessesByName("java")) {
      try { if (p.MainModule.FileName.StartsWith(@"E:\dev\bazel", StringComparison.OrdinalIgnoreCase)) { p.Kill(); log.WriteLine("killed java " + p.Id); } } catch (Exception e) { log.WriteLine("kill failed " + e.Message); }
    }
    foreach (var n in new[] { "bazel", "bazelisk" }) foreach (var p in Process.GetProcessesByName(n)) { try { p.Kill(); log.WriteLine("killed " + n + " " + p.Id); } catch { } }
    log.Flush();
  }

  public static void Run(string logPath, string stopFile, int maxSeconds) {
    log = new StreamWriter(logPath, false);
    var cw = GetConsoleWindow();
    log.WriteLine("watcher console hwnd={0} visible={1}", cw, cw != IntPtr.Zero && IsWindowVisible(cw));
    keep = OnEvent;
    var h1 = SetWinEventHook(0x8002, 0x8002, IntPtr.Zero, keep, 0, 0, 0);
    var h2 = SetWinEventHook(0x0003, 0x0003, IntPtr.Zero, keep, 0, 0, 0);
    var start = DateTime.Now;
    log.WriteLine("{0:HH:mm:ss.fff} START", start); log.Flush();
    MSG m;
    while (!File.Exists(stopFile) && (DateTime.Now - start).TotalSeconds < maxSeconds) {
      while (PeekMessage(out m, IntPtr.Zero, 0, 0, 1)) { TranslateMessage(ref m); DispatchMessage(ref m); }
      Thread.Sleep(15);
    }
    UnhookWinEvent(h1); UnhookWinEvent(h2);
    log.WriteLine("{0:HH:mm:ss.fff} END console_window_events={1} tripped={2}", DateTime.Now, total, tripped);
    log.Close();
  }
}
'@
Add-Type -TypeDefinition $src
$t0 = Get-Date
[ConsoleWatch]::Run($Log, $StopFile, $MaxSeconds)
$new = Get-Process conhost, OpenConsole -ErrorAction SilentlyContinue | Where-Object { $_.StartTime -gt $t0 }
Add-Content -Path $Log -Value ("new conhost/OpenConsole processes still alive at end: {0}" -f @($new).Count)
