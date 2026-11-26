# 假「鼠标穿透层」窗口（排障工具）：创建一个可见、整屏、带 WS_EX_TRANSPARENT 的顶层窗口，
# 用来复现 / 回归「视觉上不存在但 IsWindowVisible = TRUE 的穿透层抢走窗口吸附候选」。
#
# 现实案例（issue #225）：TabTip 的 ShellHandwritingCanvas 用过一次手写 / 触摸键盘后会一直停在
# visible = TRUE，尺寸整屏、Z 序排在全部真实窗口之前（exstyle 0x0A0800A8），于是窗口吸附永远高亮整屏。
# 本脚本造的窗口与它同形态（只少了 COMPOSITED 一位，与命中无关）：鼠标穿透、视觉上不存在。
# 按 Ctrl+C 结束 PowerShell 即移除。
$src = @'
using System;
using System.Runtime.InteropServices;

public class FakeTip {
    [StructLayout(LayoutKind.Sequential)]
    public struct WNDCLASSEX {
        public uint cbSize; public uint style; public IntPtr lpfnWndProc;
        public int cbClsExtra; public int cbWndExtra; public IntPtr hInstance;
        public IntPtr hIcon; public IntPtr hCursor; public IntPtr hbrBackground;
        public string lpszMenuName; public string lpszClassName; public IntPtr hIconSm;
    }
    [StructLayout(LayoutKind.Sequential)]
    public struct MSG {
        public IntPtr hwnd; public uint message; public IntPtr wParam;
        public IntPtr lParam; public uint time; public int x; public int y;
    }
    public delegate IntPtr WndProcDelegate(IntPtr h, uint msg, IntPtr w, IntPtr l);

    [DllImport("user32.dll")] public static extern ushort RegisterClassEx(ref WNDCLASSEX wc);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr CreateWindowExW(uint exStyle, string cls, string title, uint style,
        int x, int y, int w, int h, IntPtr parent, IntPtr menu, IntPtr inst, IntPtr param);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern int GetMessage(out MSG m, IntPtr h, uint a, uint b);
    [DllImport("user32.dll")] public static extern bool TranslateMessage(ref MSG m);
    [DllImport("user32.dll")] public static extern IntPtr DispatchMessage(ref MSG m);
    [DllImport("user32.dll")] public static extern IntPtr DefWindowProc(IntPtr h, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr GetModuleHandleW(string name);

    static WndProcDelegate _proc = new WndProcDelegate(Impl);
    static IntPtr Impl(IntPtr h, uint msg, IntPtr w, IntPtr l) { return DefWindowProc(h, msg, w, l); }

    public static IntPtr Create() {
        var wc = new WNDCLASSEX();
        wc.cbSize = (uint)Marshal.SizeOf(typeof(WNDCLASSEX));
        wc.lpfnWndProc = Marshal.GetFunctionPointerForDelegate(_proc);
        wc.hInstance = GetModuleHandleW(null);
        wc.lpszClassName = "ShellHandwritingCanvasFake";
        RegisterClassEx(ref wc);
        int x = GetSystemMetrics(76), y = GetSystemMetrics(77);
        int w = GetSystemMetrics(78), h = GetSystemMetrics(79);
        uint ex = 0x20 | 0x8 | 0x80 | 0x80000 | 0x8000000;
        IntPtr hwnd = CreateWindowExW(ex, "ShellHandwritingCanvasFake", "", 0x80000000,
            x, y, w, h, IntPtr.Zero, IntPtr.Zero, wc.hInstance, IntPtr.Zero);
        if (hwnd == IntPtr.Zero) return IntPtr.Zero;
        ShowWindow(hwnd, 4);
        return hwnd;
    }

    public static void Loop() {
        MSG m;
        while (GetMessage(out m, IntPtr.Zero, 0, 0) > 0) {
            TranslateMessage(ref m);
            DispatchMessage(ref m);
        }
    }
}
'@
Add-Type -TypeDefinition $src -Language CSharp
$h = [FakeTip]::Create()
if ($h -eq [IntPtr]::Zero) { Write-Host "创建失败" -ForegroundColor Red; exit 1 }
Write-Host ("已创建假穿透层窗口 hwnd=0x{0:X}（整屏 + WS_EX_TRANSPARENT）" -f $h.ToInt64())
Write-Host "现在按 F1 截图并悬停任意窗口：正常应高亮光标下的真实窗口，而不是整屏"
Write-Host "按 Ctrl+C 结束本脚本即移除该窗口"
[FakeTip]::Loop()
