using System;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows;

namespace MonitorHistoryViewer;

public class SingleInstanceHelper
{
    private static Mutex? _mutex;
    private const string MutexName = "MonitorHistoryViewer_SingleInstance_Mutex";

    [DllImport("user32.dll")]
    private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    private static extern bool IsWindowVisible(IntPtr hWnd);

    private const int SW_RESTORE = 9;

    public static bool EnsureSingleInstance()
    {
        _mutex = new Mutex(true, MutexName, out bool createdNew);

        if (!createdNew)
        {
            ShowExistingInstance();
            return false;
        }

        return true;
    }

    private static void ShowExistingInstance()
    {
        var currentProcess = System.Diagnostics.Process.GetCurrentProcess();
        var processes = System.Diagnostics.Process.GetProcessesByName(currentProcess.ProcessName);

        foreach (var process in processes)
        {
            if (process.Id != currentProcess.Id)
            {
                var hWnd = process.MainWindowHandle;
                if (hWnd != IntPtr.Zero)
                {
                    if (!IsWindowVisible(hWnd))
                    {
                        ShowWindow(hWnd, SW_RESTORE);
                    }
                    SetForegroundWindow(hWnd);
                }
                break;
            }
        }
    }

    public static void ReleaseMutex()
    {
        if (_mutex != null)
        {
            try
            {
                _mutex.ReleaseMutex();
            }
            catch { }
            _mutex.Dispose();
            _mutex = null;
        }
    }
}