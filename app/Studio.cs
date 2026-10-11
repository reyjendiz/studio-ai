// Native window for the Studio UI: starts ComfyUI + the UI server hidden, hosts the page in WebView2, stops what it started on close.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;

static class Program
{
    [STAThread]
    static void Main()
    {
        bool first;
        using (var mutex = new Mutex(true, "Local\\StudioApp", out first))
        {
            if (!first) return;   // one window at a time: a second launch would fight over the ports
            Application.EnableVisualStyles();
            Application.Run(new MainForm());
        }
    }
}

class MainForm : Form
{
    const string Url = "http://127.0.0.1:8189";
    readonly string root = Path.GetDirectoryName(Path.GetDirectoryName(Application.ExecutablePath));
    readonly List<Process> started = new List<Process>();   // only what we launched gets stopped
    readonly WebView2 web = new WebView2();
    readonly Label msg = new Label();
    readonly bool dark = true;   // the page is always dark now, so the window frame and loading screen are too

    [DllImport("dwmapi.dll")]
    static extern int DwmSetWindowAttribute(IntPtr hwnd, int attr, ref int value, int size);

    public MainForm()
    {
        Text = "Студия";
        ClientSize = new Size(1280, 840);
        MinimumSize = new Size(900, 600);
        StartPosition = FormStartPosition.CenterScreen;
        WindowState = FormWindowState.Maximized;
        try { Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { }
        BackColor = dark ? Color.FromArgb(4, 5, 4) : Color.FromArgb(245, 245, 247);

        msg.Dock = DockStyle.Fill;
        msg.TextAlign = ContentAlignment.MiddleCenter;
        msg.Font = new Font("Segoe UI", 12f);
        msg.ForeColor = dark ? Color.Silver : Color.DimGray;
        msg.Text = "Запуск…";
        web.Dock = DockStyle.Fill;
        web.Visible = false;
        web.DefaultBackgroundColor = BackColor;
        Controls.Add(web);
        Controls.Add(msg);

        Load += async (s, e) => await Boot();
        FormClosing += (s, e) => Shutdown();
    }

    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        int on = dark ? 1 : 0;
        DwmSetWindowAttribute(Handle, 20, ref on, 4);   // dark title bar to match the page
    }

    async Task Boot()
    {
        try
        {
            await Task.Run(() => StartServers());
            var env = await CoreWebView2Environment.CreateAsync(null, Path.Combine(root, @"ui\wv2profile"));
            await web.EnsureCoreWebView2Async(env);
            web.CoreWebView2.Settings.AreDevToolsEnabled = false;
            web.CoreWebView2.Settings.IsStatusBarEnabled = false;
            web.CoreWebView2.NavigationCompleted += (s, e) => { web.Visible = true; msg.Visible = false; };
            web.CoreWebView2.Navigate(Url);
        }
        catch (WebView2RuntimeNotFoundException)
        {
            msg.Text = "Не найден Microsoft Edge WebView2 Runtime.\nУстанови его с microsoft.com/edge/webview2";
        }
        catch (Exception ex)
        {
            msg.Text = "Не удалось запустить: " + ex.Message;
        }
    }

    void StartServers()
    {
        // ComfyUI is not started here: the UI server launches it on the first generation and stops it when idle
        if (!PortOpen(8189)) started.Add(Launch("ui", @"ui\server.py"));
        for (int i = 0; i < 120 && !PortOpen(8189); i++) Thread.Sleep(500);
        if (!PortOpen(8189)) throw new Exception("сервер интерфейса не запустился, смотри папку logs");
    }

    // python.exe hidden, output redirected by cmd into logs (an unread redirected pipe would eventually block the process)
    Process Launch(string log, string args)
    {
        string logs = Path.Combine(root, "logs");
        Directory.CreateDirectory(logs);
        string py = Path.Combine(root, @"python_embeded\python.exe");
        var psi = new ProcessStartInfo("cmd.exe", "/c \"\"" + py + "\" -s " + args + " > \"" + Path.Combine(logs, log + ".out.log") + "\" 2>&1\"")
        {
            WorkingDirectory = root,
            UseShellExecute = false,
            CreateNoWindow = true
        };
        return Process.Start(psi);
    }

    void Shutdown()
    {
        foreach (var p in started)
        {
            try
            {
                var k = Process.Start(new ProcessStartInfo("taskkill", "/T /F /PID " + p.Id) { UseShellExecute = false, CreateNoWindow = true });
                k.WaitForExit(5000);
            }
            catch { }
        }
        // also stop servers that were already running when the window opened (UI server and ComfyUI), so nothing outlives the app
        try
        {
            var k = Process.Start(new ProcessStartInfo("powershell", "-NoProfile -Command \"Get-NetTCPConnection -LocalPort 8188,8189 -State Listen -ErrorAction SilentlyContinue | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }\"") { UseShellExecute = false, CreateNoWindow = true });
            k.WaitForExit(8000);
        }
        catch { }
    }

    static bool PortOpen(int port)
    {
        using (var c = new TcpClient())
        {
            try
            {
                var r = c.BeginConnect("127.0.0.1", port, null, null);
                return r.AsyncWaitHandle.WaitOne(300) && c.Connected;
            }
            catch { return false; }
        }
    }

    static bool IsDarkTheme()
    {
        try
        {
            var v = Microsoft.Win32.Registry.GetValue(@"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "AppsUseLightTheme", 1);
            return v is int && (int)v == 0;
        }
        catch { return false; }
    }
}
